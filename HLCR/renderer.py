import json
import os
import ctypes
from pathlib import Path
from typing import Dict, List, Any, Optional
from PySide6.QtCore import QThread, Signal, QObject, QProcess

CONFIG_FILE = "config.json"

CODEC_PRESETS = {
    "prores": {
        "standard": ["-c:v", "prores", "-profile:v", "3", "-pix_fmt", "yuv422p10le"],
        "alpha": ["-c:v", "prores_ks", "-profile:v", "4444", "-pix_fmt", "yuva444p10le"],
        "ext_standard": ".mov",
        "ext_alpha": ".mov"
    },
    "h264": {
        "standard": ["-c:v", "libx264", "-preset", "fast", "-crf", "16", "-pix_fmt", "yuv420p"],
        "alpha": ["-c:v", "prores_ks", "-profile:v", "4444", "-pix_fmt", "yuva444p10le"],
        "ext_standard": ".mp4",
        "ext_alpha": ".mov"
    },
    "dnxhr": {
        "standard": ["-c:v", "dnxhd", "-profile:v", "dnxhr_hq", "-pix_fmt", "yuv422p"],
        "alpha": ["-c:v", "dnxhd", "-profile:v", "dnxhr_444", "-pix_fmt", "yuv444p10le"],
        "ext_standard": ".mov",
        "ext_alpha": ".mov"
    }
}

def load_config() -> Dict[str, Any]:
    """Reads the JSON config file. Creates a default one if it doesn't exist."""
    default_config = {
        "ffmpeg_path": "ffmpeg",
        "source_folder": "",
        "output_folder": "",
        "fps": 300,
        "codec": "prores",
        "max_concurrent_renders": 2
    }

    if not os.path.exists(CONFIG_FILE):
        print(f"'{CONFIG_FILE}' not found. Creating a new default configuration.")
        save_config(default_config)
        return default_config

    try:
        with open(CONFIG_FILE, 'r', encoding='utf-8') as f:
            data = json.load(f)
            # Merge keys to ensure default keys exist
            for k, v in default_config.items():
                if k not in data:
                    data[k] = v
            return data
    except Exception:
        return default_config

def save_config(config_data: Dict[str, Any]) -> None:
    """Saves the given dictionary to the JSON config file."""
    with open(CONFIG_FILE, 'w', encoding='utf-8') as f:
        json.dump(config_data, f, indent=4)

class FolderScanner(QThread):
    # Signals to communicate with the main UI thread during background scanning
    clip_found = Signal(dict)
    progress_scan = Signal(str)
    finished_scan = Signal(int)

    def __init__(self, source_folder: Path):
        super().__init__()
        self.source_folder = source_folder
        self.is_cancelled = False

    def cancel(self) -> None:
        self.is_cancelled = True

    def run(self) -> None:
        try:
            total = self._scan()
            self.finished_scan.emit(total)
        except Exception as e:
            self.progress_scan.emit(f"Scan error: {str(e)}")
            self.finished_scan.emit(0)

    def _scan(self) -> int:
        processed_folders = set()
        count = 0

        # Recursively walk source folder efficiently using os.walk
        for root, dirs, files in os.walk(str(self.source_folder)):
            if self.is_cancelled:
                return count

            take_folder = Path(root)
            if take_folder in processed_folders:
                continue

            # Identify wav files in the current directory
            wav_files = [f for f in files if f.lower().endswith('.wav')]
            if not wav_files:
                continue

            # Scan subdirectories for HLAE frame outputs
            image_folders = []
            try:
                for entry in os.scandir(take_folder):
                    if self.is_cancelled:
                        return count
                    if entry.is_dir():
                        bmp_check = Path(entry.path) / "00000.bmp"
                        if bmp_check.exists():
                            image_folders.append(Path(entry.path))
            except OSError:
                continue

            if not image_folders:
                continue

            # Valid take found!
            processed_folders.add(take_folder)
            self.progress_scan.emit(f"Found take: {take_folder.name}")

            # Prioritize sound.wav if it exists
            wav_to_use = take_folder / "sound.wav"
            if not wav_to_use.exists():
                wav_to_use = take_folder / wav_files[0]

            wav_name = wav_to_use.stem
            demo_name = take_folder.parent.name
            take_name = take_folder.name

            if wav_name.lower() == "sound":
                base_name = f"{demo_name}-{take_name}-{wav_name}"
            else:
                base_name = wav_name

            folder_names = {f.name: f for f in image_folders}

            # Bundle HLAE split streams if all three exist
            if "all" in folder_names and "hudcolor" in folder_names and "hudalpha" in folder_names:
                # Fast BMP count using os.scandir instead of glob
                frame_count = sum(1 for entry in os.scandir(folder_names["all"]) if entry.is_file() and entry.name.lower().endswith('.bmp'))
                
                self.clip_found.emit({
                    "take_folder": str(take_folder),
                    "type": "single",
                    "img_folder": "all",
                    "wav_file": wav_to_use.name,
                    "base_name": base_name,
                    "frame_count": frame_count
                })
                count += 1

                self.clip_found.emit({
                    "take_folder": str(take_folder),
                    "type": "hud_only",
                    "img_folder": "hudcolor",
                    "wav_file": wav_to_use.name,
                    "base_name": base_name,
                    "frame_count": frame_count
                })
                count += 1

                for name in ["all", "hudcolor", "hudalpha"]:
                    image_folders.remove(folder_names[name])

            # Process any remaining image folders normally
            for img_folder in image_folders:
                if self.is_cancelled:
                    return count
                frame_count = sum(1 for entry in os.scandir(img_folder) if entry.is_file() and entry.name.lower().endswith('.bmp'))
                self.clip_found.emit({
                    "take_folder": str(take_folder),
                    "type": "single",
                    "img_folder": img_folder.name,
                    "wav_file": wav_to_use.name,
                    "base_name": base_name,
                    "frame_count": frame_count
                })
                count += 1

        return count

class RenderJob(QObject):
    # Signals for asynchronous updates
    progress_changed = Signal(str, int)  # job_id, percent
    speed_changed = Signal(str, str)     # job_id, speed_str (e.g. "250 fps (0.8x)")
    status_changed = Signal(str, str)    # job_id, status_text
    finished = Signal(str, bool, str)    # job_id, success, error_msg

    def __init__(self, job_id: str, clip: Dict[str, Any], config: Dict[str, Any]):
        super().__init__()
        self.job_id = job_id
        self.clip = clip
        self.config = config
        self.process: Optional[QProcess] = None
        self.is_cancelled = False
        
        self.current_fps = "0"
        self.current_speed = "0x"
        self.stdout_buffer = ""

    def start(self) -> None:
        ffmpeg_path = Path(self.config.get("ffmpeg_path", "ffmpeg"))
        fps = str(self.config.get("fps", 300))
        codec_name = self.config.get("codec", "prores")

        take_folder = Path(self.clip["take_folder"])
        wav_file = take_folder / self.clip["wav_file"]

        if not ffmpeg_path.exists() or not ffmpeg_path.is_file():
            self.finished.emit(self.job_id, False, f"FFmpeg not found at: {ffmpeg_path}")
            return
        if not take_folder.exists() or not take_folder.is_dir():
            self.finished.emit(self.job_id, False, f"Take folder not found: {take_folder}")
            return
        if not wav_file.exists():
            self.finished.emit(self.job_id, False, f"Audio file not found: {wav_file}")
            return

        preset = CODEC_PRESETS.get(codec_name, CODEC_PRESETS["prores"])
        clip_type = self.clip.get("type", "single")

        output_folder = Path(self.config.get("output_folder", ""))
        output_folder.mkdir(parents=True, exist_ok=True)

        if clip_type == "hud_only":
            codec_args = preset["alpha"]
            ext = preset["ext_alpha"]
            base_out = f"{self.clip['base_name']}-hud"
        else:
            codec_args = preset["standard"]
            ext = preset["ext_standard"]
            base_out = f"{self.clip['base_name']}-{self.clip['img_folder']}"

        final_name = self._get_unique_filename(output_folder, base_out, ext)
        out_file = output_folder / final_name

        # Calculate thread scaling to prevent CPU thrashing
        max_concurrent = int(self.config.get("max_concurrent_renders", 2))
        try:
            cpu_cores = os.cpu_count() or 4
            threads_per_process = max(1, cpu_cores // max_concurrent)
        except Exception:
            threads_per_process = 2

        cmd_args = ["-y"]

        if clip_type == "hud_only":
            cmd_args += [
                "-framerate", fps, "-i", "hudcolor/%05d.bmp",
                "-framerate", fps, "-i", "hudalpha/%05d.bmp",
                "-i", self.clip["wav_file"],
                "-filter_complex", "[1:v]extractplanes=r[alpha];[0:v][alpha]alphamerge[hud]",
                "-map", "[hud]", "-map", "2:a"
            ]
        else:
            cmd_args += [
                "-framerate", fps,
                "-i", f"{self.clip['img_folder']}/%05d.bmp",
                "-i", self.clip["wav_file"]
            ]

        cmd_args += codec_args
        cmd_args += [
            "-threads", str(threads_per_process),
            "-c:a", "pcm_s16le", "-shortest", "-movflags", "+faststart",
            "-progress", "pipe:1", "-loglevel", "error",
            str(out_file)
        ]

        self.process = QProcess(self)
        self.process.setWorkingDirectory(str(take_folder))
        
        self.process.readyReadStandardOutput.connect(self.handle_stdout)
        self.process.finished.connect(self.handle_finished)
        self.process.errorOccurred.connect(self.handle_error)

        self.status_changed.emit(self.job_id, "Rendering")
        self.process.start(str(ffmpeg_path), cmd_args)

    def handle_stdout(self) -> None:
        if not self.process:
            return
        data = self.process.readAllStandardOutput().data().decode("utf-8", errors="ignore")
        self.stdout_buffer += data

        while "\n" in self.stdout_buffer:
            line, self.stdout_buffer = self.stdout_buffer.split("\n", 1)
            line = line.strip()
            if not line or "=" not in line:
                continue

            key, val = line.split("=", 1)
            key, val = key.strip(), val.strip()

            if key == "frame":
                try:
                    current_frame = int(val)
                    total_frames = self.clip.get("frame_count", 0)
                    if total_frames > 0:
                        percent = min(100, int((current_frame / total_frames) * 100))
                        self.progress_changed.emit(self.job_id, percent)
                except ValueError:
                    pass
            elif key == "fps":
                self.current_fps = val
                self._emit_speed()
            elif key == "speed":
                self.current_speed = val
                self._emit_speed()

    def _emit_speed(self) -> None:
        self.speed_changed.emit(self.job_id, f"{self.current_fps} fps ({self.current_speed})")

    def handle_finished(self, exit_code: int, exit_status: QProcess.ExitStatus) -> None:
        if self.is_cancelled:
            self.status_changed.emit(self.job_id, "Cancelled")
            self.finished.emit(self.job_id, False, "Cancelled by user")
            return

        if exit_code == 0 and exit_status == QProcess.NormalExit:
            self.status_changed.emit(self.job_id, "Finished")
            self.progress_changed.emit(self.job_id, 100)
            self.finished.emit(self.job_id, True, "")
        else:
            err = ""
            if self.process:
                err = self.process.readAllStandardError().data().decode("utf-8", errors="ignore")
            if not err:
                err = f"Exit code: {exit_code}"
            self.status_changed.emit(self.job_id, "Error")
            self.finished.emit(self.job_id, False, err)

    def handle_error(self, error: QProcess.ProcessError) -> None:
        if self.is_cancelled:
            return
        err_msg = f"QProcess error: {error.name if hasattr(error, 'name') else str(error)}"
        self.status_changed.emit(self.job_id, "Error")
        self.finished.emit(self.job_id, False, err_msg)

    def cancel(self) -> None:
        self.is_cancelled = True
        if self.process and self.process.state() != QProcess.ProcessState.NotRunning:
            self.process.terminate()
            if not self.process.waitForFinished(2000):
                self.process.kill()

    def _get_unique_filename(self, output_dir: Path, base_name: str, ext: str) -> str:
        counter = 1
        final_name = f"{base_name}{ext}"
        while (output_dir / final_name).exists():
            final_name = f"{base_name}_{counter}{ext}"
            counter += 1
        return final_name