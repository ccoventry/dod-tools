import sys
import os
from pathlib import Path
from typing import Dict, List, Any, Optional

from PySide6.QtWidgets import (QApplication, QMainWindow, QWidget, QVBoxLayout, 
                               QHBoxLayout, QLabel, QLineEdit, QPushButton, 
                               QFileDialog, QSpinBox, QProgressBar, QMessageBox,
                               QTableWidget, QTableWidgetItem, QHeaderView, QFrame,
                               QComboBox, QGridLayout)
from PySide6.QtCore import Qt, QSize
from PySide6.QtGui import QColor, QCloseEvent, QIcon

from renderer import load_config, save_config, FolderScanner, RenderJob

QSS_STYLE = """
QMainWindow {
    background-color: #1e1e2e;
}
QWidget {
    color: #cdd6f4;
    font-family: 'Segoe UI', Arial, sans-serif;
    font-size: 13px;
}
QFrame#config_frame {
    background-color: #181825;
    border: 1px solid #313244;
    border-radius: 8px;
}
QLabel {
    font-weight: 500;
}
QLineEdit, QSpinBox, QComboBox {
    background-color: #11111b;
    border: 1px solid #313244;
    border-radius: 6px;
    padding: 6px 10px;
    color: #cdd6f4;
}
QLineEdit:focus, QSpinBox:focus, QComboBox:focus {
    border: 1px solid #89b4fa;
}
QPushButton {
    background-color: #313244;
    border: 1px solid #45475a;
    border-radius: 6px;
    padding: 6px 14px;
    font-weight: bold;
    color: #cdd6f4;
}
QPushButton:hover {
    background-color: #45475a;
    border: 1px solid #585b70;
}
QPushButton:pressed {
    background-color: #181825;
}
QPushButton:disabled {
    background-color: #11111b;
    border: 1px solid #181825;
    color: #585b70;
}
QPushButton#start_btn {
    background-color: #89b4fa;
    color: #11111b;
    border: 1px solid #74c7ec;
}
QPushButton#start_btn:hover {
    background-color: #b4befe;
}
QPushButton#start_btn:disabled {
    background-color: #313244;
    color: #585b70;
    border: 1px solid #45475a;
}
QProgressBar {
    border: 1px solid #313244;
    border-radius: 6px;
    background-color: #11111b;
    text-align: center;
    font-weight: bold;
    color: #cdd6f4;
}
QProgressBar::chunk {
    background-color: qlineargradient(x1:0, y1:0, x2:1, y2:0, stop:0 #89b4fa, stop:1 #b4befe);
    border-radius: 5px;
}
QTableWidget {
    background-color: #181825;
    border: 1px solid #313244;
    border-radius: 8px;
    gridline-color: #313244;
}
QHeaderView::section {
    background-color: #11111b;
    color: #cdd6f4;
    padding: 6px;
    border: 1px solid #313244;
    font-weight: bold;
}
QTableWidget::item {
    padding: 6px;
}
QScrollBar:vertical {
    border: none;
    background: #181825;
    width: 10px;
    margin: 0px;
}
QScrollBar::handle:vertical {
    background: #45475a;
    min-height: 20px;
    border-radius: 5px;
}
QScrollBar::handle:vertical:hover {
    background: #585b70;
}
"""

class MainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("HLCR - Parallel Clip Renderer")
        self.setMinimumSize(850, 600)
        self.setStyleSheet(QSS_STYLE)
        
        self.config = load_config()
        self.clips: List[Dict[str, Any]] = []
        self.active_jobs: Dict[str, RenderJob] = {}
        self.scanner: Optional[FolderScanner] = None
        self.is_rendering = False
        
        self.setup_ui()
        self.load_settings_to_ui()
        
        # Auto-scan if directories are already populated on startup
        if self.config.get("source_folder"):
            self.start_scan()

    def setup_ui(self):
        central_widget = QWidget()
        self.setCentralWidget(central_widget)
        main_layout = QVBoxLayout(central_widget)
        main_layout.setContentsMargins(15, 15, 15, 15)
        main_layout.setSpacing(12)
        
        # 1. Config panel
        config_frame = QFrame()
        config_frame.setObjectName("config_frame")
        config_layout = QGridLayout(config_frame)
        config_layout.setContentsMargins(12, 12, 12, 12)
        config_layout.setSpacing(8)
        
        config_layout.addWidget(QLabel("FFmpeg Path:"), 0, 0)
        self.ffmpeg_input = QLineEdit()
        config_layout.addWidget(self.ffmpeg_input, 0, 1)
        btn_ffmpeg = QPushButton("Browse")
        btn_ffmpeg.clicked.connect(lambda: self.browse_path(self.ffmpeg_input, is_file=True))
        config_layout.addWidget(btn_ffmpeg, 0, 2)
        
        config_layout.addWidget(QLabel("Source Folder:"), 1, 0)
        self.source_input = QLineEdit()
        config_layout.addWidget(self.source_input, 1, 1)
        btn_source = QPushButton("Browse")
        btn_source.clicked.connect(lambda: self.browse_path(self.source_input, is_file=False))
        config_layout.addWidget(btn_source, 1, 2)
        
        config_layout.addWidget(QLabel("Output Folder:"), 2, 0)
        self.output_input = QLineEdit()
        config_layout.addWidget(self.output_input, 2, 1)
        btn_output = QPushButton("Browse")
        btn_output.clicked.connect(lambda: self.browse_path(self.output_input, is_file=False))
        config_layout.addWidget(btn_output, 2, 2)
        
        # Codec / FPS / Concurrency Settings Row
        controls_layout = QHBoxLayout()
        
        controls_layout.addWidget(QLabel("Frame Rate (FPS):"))
        self.fps_spinbox = QSpinBox()
        self.fps_spinbox.setRange(1, 1000)
        self.fps_spinbox.setValue(300)
        controls_layout.addWidget(self.fps_spinbox)
        
        controls_layout.addSpacing(15)
        controls_layout.addWidget(QLabel("Codec Preset:"))
        self.codec_combobox = QComboBox()
        self.codec_combobox.addItem("ProRes (Edit-friendly)", "prores")
        self.codec_combobox.addItem("H.264 / ProRes (Space-saving)", "h264")
        self.codec_combobox.addItem("DNxHR (Avid Edit-friendly)", "dnxhr")
        self.codec_combobox.setToolTip(
            "• ProRes: Best scrubbing performance in Premiere/Resolve. Large files.\n"
            "• H.264: Low disk space, but falls back to ProRes 4444 for HUDs to retain alpha.\n"
            "• DNxHR: Avid standard, highly optimized for Vegas Pro / Windows."
        )
        controls_layout.addWidget(self.codec_combobox)
        
        controls_layout.addSpacing(15)
        controls_layout.addWidget(QLabel("Max Concurrent Renders:"))
        self.concurrent_spinbox = QSpinBox()
        self.concurrent_spinbox.setRange(1, 8)
        self.concurrent_spinbox.setValue(2)
        self.concurrent_spinbox.setToolTip(
            "Suggested thread limit based on storage:\n"
            "• 1-2 renders: Traditional HDD\n"
            "• 2-3 renders: SATA SSD\n"
            "• 4+ renders: NVMe SSD"
        )
        controls_layout.addWidget(self.concurrent_spinbox)
        controls_layout.addStretch()
        
        config_layout.addLayout(controls_layout, 3, 1, 1, 2)
        main_layout.addWidget(config_frame)
        
        # 2. Render Queue Table
        self.table = QTableWidget()
        self.table.setColumnCount(7)
        self.table.setHorizontalHeaderLabels([
            "Clip Name", "Stream/Folder", "Frames", "Status", "Speed", "Progress", "Actions"
        ])
        self.table.setAlternatingRowColors(True)
        self.table.setSelectionBehavior(QTableWidget.SelectionBehavior.SelectRows)
        self.table.setEditTriggers(QTableWidget.EditTrigger.NoEditTriggers)
        
        header = self.table.horizontalHeader()
        header.setSectionResizeMode(0, QHeaderView.Stretch)
        header.setSectionResizeMode(1, QHeaderView.ResizeToContents)
        header.setSectionResizeMode(2, QHeaderView.ResizeToContents)
        header.setSectionResizeMode(3, QHeaderView.ResizeToContents)
        header.setSectionResizeMode(4, QHeaderView.ResizeToContents)
        header.setSectionResizeMode(5, QHeaderView.Stretch)
        header.setSectionResizeMode(6, QHeaderView.ResizeToContents)
        
        main_layout.addWidget(self.table)
        
        # 3. Status Bar & Controls
        footer_layout = QHBoxLayout()
        self.status_label = QLabel("Idle / Waiting for Scan")
        self.status_label.setWordWrap(True)
        footer_layout.addWidget(self.status_label)
        footer_layout.addStretch()
        
        self.global_progress_bar = QProgressBar()
        self.global_progress_bar.setFixedWidth(200)
        self.global_progress_bar.setValue(0)
        footer_layout.addWidget(self.global_progress_bar)
        
        self.btn_scan = QPushButton("Scan Folder")
        self.btn_scan.clicked.connect(self.start_scan)
        footer_layout.addWidget(self.btn_scan)
        
        self.btn_cancel = QPushButton("Cancel All")
        self.btn_cancel.clicked.connect(self.cancel_all)
        self.btn_cancel.setEnabled(False)
        footer_layout.addWidget(self.btn_cancel)
        
        self.btn_start = QPushButton("Start Render")
        self.btn_start.setObjectName("start_btn")
        self.btn_start.clicked.connect(self.start_rendering)
        self.btn_start.setEnabled(False)
        footer_layout.addWidget(self.btn_start)
        
        main_layout.addLayout(footer_layout)

    def load_settings_to_ui(self):
        self.ffmpeg_input.setText(self.config.get("ffmpeg_path", ""))
        self.source_input.setText(self.config.get("source_folder", ""))
        self.output_input.setText(self.config.get("output_folder", ""))
        self.fps_spinbox.setValue(int(self.config.get("fps", 300)))
        
        codec_val = self.config.get("codec", "prores")
        idx = self.codec_combobox.findData(codec_val)
        if idx != -1:
            self.codec_combobox.setCurrentIndex(idx)
            
        self.concurrent_spinbox.setValue(int(self.config.get("max_concurrent_renders", 2)))

    def save_settings_from_ui(self):
        self.config["ffmpeg_path"] = self.ffmpeg_input.text().strip()
        self.config["source_folder"] = self.source_input.text().strip()
        self.config["output_folder"] = self.output_input.text().strip()
        self.config["fps"] = self.fps_spinbox.value()
        self.config["codec"] = self.codec_combobox.currentData()
        self.config["max_concurrent_renders"] = self.concurrent_spinbox.value()
        save_config(self.config)

    def browse_path(self, line_edit: QLineEdit, is_file: bool = False):
        if is_file:
            path, _ = QFileDialog.getOpenFileName(self, "Select FFmpeg Executable")
        else:
            path = QFileDialog.getExistingDirectory(self, "Select Folder")
            
        if path:
            line_edit.setText(path)
            self.save_settings_from_ui()
            # If changing folders, auto-trigger a scan
            if line_edit == self.source_input:
                self.start_scan()

    def start_scan(self):
        self.save_settings_from_ui()
        source_dir = Path(self.config.get("source_folder", ""))
        
        if not source_dir.exists() or not source_dir.is_dir():
            self.status_label.setText("Idle / No valid source directory")
            return
            
        # Stop scanner if active
        if self.scanner and self.scanner.isRunning():
            self.scanner.cancel()
            self.scanner.wait()

        # Reset clips list & table
        self.clips.clear()
        self.table.setRowCount(0)
        self.btn_start.setEnabled(False)
        self.btn_scan.setEnabled(False)
        self.status_label.setText("Scanning source folder...")
        
        self.scanner = FolderScanner(source_dir)
        self.scanner.clip_found.connect(self.on_clip_found)
        self.scanner.progress_scan.connect(self.status_label.setText)
        self.scanner.finished_scan.connect(self.on_scan_finished)
        self.scanner.start()

    def on_clip_found(self, clip: dict):
        self.clips.append(clip)
        row = self.table.rowCount()
        self.table.insertRow(row)
        
        # Col 0: Base Name
        self.table.setItem(row, 0, QTableWidgetItem(clip["base_name"]))
        # Col 1: Stream type
        self.table.setItem(row, 1, QTableWidgetItem(clip["type"].upper() if clip["type"] == "hud_only" else clip["img_folder"]))
        # Col 2: Frames count
        self.table.setItem(row, 2, QTableWidgetItem(str(clip["frame_count"])))
        
        # Col 3: Status
        status_item = QTableWidgetItem("Queued")
        status_item.setTextAlignment(Qt.AlignCenter)
        self.table.setItem(row, 3, status_item)
        
        # Col 4: Speed
        speed_item = QTableWidgetItem("")
        speed_item.setTextAlignment(Qt.AlignCenter)
        self.table.setItem(row, 4, speed_item)
        
        # Col 5: Progress Bar
        progress_bar = QProgressBar()
        progress_bar.setRange(0, 100)
        progress_bar.setValue(0)
        progress_bar.setStyleSheet("QProgressBar { height: 16px; margin: 2px; }")
        self.table.setCellWidget(row, 5, progress_bar)
        
        # Col 6: Actions cancel individual button
        cancel_btn = QPushButton("✖")
        cancel_btn.setToolTip("Cancel this render job")
        cancel_btn.setStyleSheet("QPushButton { padding: 2px 8px; font-weight: bold; border-radius: 4px; }")
        cancel_btn.setEnabled(False)
        cancel_btn.clicked.connect(self.on_cancel_btn_clicked)
        self.table.setCellWidget(row, 6, cancel_btn)
        
        self.status_label.setText(f"Scanning: found {len(self.clips)} clips...")

    def on_scan_finished(self, total_count: int):
        self.btn_scan.setEnabled(True)
        if total_count > 0:
            self.btn_start.setEnabled(True)
            self.status_label.setText(f"Scan complete. Discovered {total_count} clips ready to render.")
        else:
            self.status_label.setText("Scan complete. No clips found.")
        self.scanner = None

    def start_rendering(self):
        self.save_settings_from_ui()
        
        # Reset completed status colors and values
        for row in range(self.table.rowCount()):
            status_item = self.table.item(row, 3)
            if status_item and status_item.text() in ["Finished", "Error", "Cancelled"]:
                status_item.setText("Queued")
                status_item.setForeground(QColor("#cdd6f4"))
                pbar = self.table.cellWidget(row, 5)
                if isinstance(pbar, QProgressBar):
                    pbar.setValue(0)
                speed_item = self.table.item(row, 4)
                if speed_item:
                    speed_item.setText("")

        self.is_rendering = True
        self.btn_start.setEnabled(False)
        self.btn_cancel.setEnabled(True)
        self.btn_scan.setEnabled(False)
        self.global_progress_bar.setValue(0)
        
        self.status_label.setText("Starting parallel render queue...")
        self._schedule_more_jobs()

    def _schedule_more_jobs(self):
        if not self.is_rendering:
            return
            
        active_count = len(self.active_jobs)
        max_concurrent = self.concurrent_spinbox.value()
        
        if active_count >= max_concurrent:
            return
            
        for row in range(self.table.rowCount()):
            if len(self.active_jobs) >= max_concurrent:
                break
                
            status_item = self.table.item(row, 3)
            if status_item and status_item.text() == "Queued":
                clip = self.clips[row]
                job_id = str(row)
                
                # Instatiate Job
                job = RenderJob(job_id, clip, self.config)
                job.progress_changed.connect(self.on_job_progress)
                job.speed_changed.connect(self.on_job_speed)
                job.status_changed.connect(self.on_job_status)
                job.finished.connect(self.on_job_finished)
                
                self.active_jobs[job_id] = job
                
                # Update actions column button
                cancel_btn = self.table.cellWidget(row, 6)
                if cancel_btn:
                    cancel_btn.setEnabled(True)
                
                if len(self.active_jobs) == 1:
                    self.prevent_sleep(True)
                    
                job.start()

    def on_job_progress(self, job_id: str, percent: int):
        row = int(job_id)
        pbar = self.table.cellWidget(row, 5)
        if isinstance(pbar, QProgressBar):
            pbar.setValue(percent)
        self.update_global_progress()

    def on_job_speed(self, job_id: str, speed_str: str):
        row = int(job_id)
        speed_item = self.table.item(row, 4)
        if speed_item:
            speed_item.setText(speed_str)

    def on_job_status(self, job_id: str, status_str: str):
        row = int(job_id)
        item = self.table.item(row, 3)
        if item:
            item.setText(status_str)
            if status_str == "Finished":
                item.setForeground(QColor("#a6e3a1"))
            elif status_str == "Error":
                item.setForeground(QColor("#f38ba8"))
            elif status_str == "Cancelled":
                item.setForeground(QColor("#f9e2af"))
            elif status_str == "Rendering":
                item.setForeground(QColor("#89b4fa"))

    def on_job_finished(self, job_id: str, success: bool, error_msg: str):
        row = int(job_id)
        
        if job_id in self.active_jobs:
            del self.active_jobs[job_id]
            
        if len(self.active_jobs) == 0:
            self.prevent_sleep(False)
            
        cancel_btn = self.table.cellWidget(row, 6)
        if cancel_btn:
            cancel_btn.setEnabled(False)
            
        if not success and error_msg and error_msg != "Cancelled by user":
            status_item = self.table.item(row, 3)
            if status_item:
                status_item.setToolTip(error_msg)
                
        # Schedule next jobs
        self._schedule_more_jobs()
        
        # Update global stats
        self.update_global_progress()
        
        # Check queue completion
        if len(self.active_jobs) == 0:
            has_queued = False
            for r in range(self.table.rowCount()):
                status_item = self.table.item(r, 3)
                if status_item and status_item.text() == "Queued":
                    has_queued = True
                    break
            if not has_queued:
                self.is_rendering = False
                self.btn_start.setEnabled(True)
                self.btn_cancel.setEnabled(False)
                self.btn_scan.setEnabled(True)
                self.status_label.setText("Render queue processing finished.")

    def on_cancel_btn_clicked(self):
        button = self.sender()
        if not button:
            return
        for row in range(self.table.rowCount()):
            if self.table.cellWidget(row, 6) == button:
                self.cancel_job(row)
                break

    def cancel_job(self, row: int):
        job_id = str(row)
        if job_id in self.active_jobs:
            self.status_label.setText(f"Cancelling clip: {self.clips[row]['base_name']}...")
            self.active_jobs[job_id].cancel()
        else:
            status_item = self.table.item(row, 3)
            if status_item and status_item.text() == "Queued":
                status_item.setText("Cancelled")
                status_item.setForeground(QColor("#f9e2af"))
                cancel_btn = self.table.cellWidget(row, 6)
                if cancel_btn:
                    cancel_btn.setEnabled(False)

    def cancel_all(self):
        self.is_rendering = False
        self.status_label.setText("Stopping queue and terminating renders...")
        
        active_ids = list(self.active_jobs.keys())
        for jid in active_ids:
            self.active_jobs[jid].cancel()
            
        for row in range(self.table.rowCount()):
            status_item = self.table.item(row, 3)
            if status_item and status_item.text() == "Queued":
                status_item.setText("Cancelled")
                status_item.setForeground(QColor("#f9e2af"))
                cancel_btn = self.table.cellWidget(row, 6)
                if cancel_btn:
                    cancel_btn.setEnabled(False)
                    
        self.btn_start.setEnabled(True)
        self.btn_cancel.setEnabled(False)
        self.btn_scan.setEnabled(True)
        self.status_label.setText("Render queue cancelled.")

    def update_global_progress(self):
        total_clips = len(self.clips)
        if total_clips == 0:
            self.global_progress_bar.setValue(0)
            return
            
        total_percent = 0
        for row in range(self.table.rowCount()):
            pbar = self.table.cellWidget(row, 5)
            if isinstance(pbar, QProgressBar):
                total_percent += pbar.value()
                
        avg_percent = int(total_percent / total_clips)
        self.global_progress_bar.setValue(avg_percent)

    def prevent_sleep(self, enable: bool):
        if os.name == 'nt':
            try:
                import ctypes
                if enable:
                    ctypes.windll.kernel32.SetThreadExecutionState(0x80000003)
                else:
                    ctypes.windll.kernel32.SetThreadExecutionState(0x80000000)
            except Exception:
                pass

    def closeEvent(self, event: QCloseEvent):
        if self.active_jobs:
            reply = QMessageBox.question(
                self, 'Exit HLCR',
                "Active render jobs are still running. Do you want to cancel them and exit?",
                QMessageBox.Yes | QMessageBox.No, QMessageBox.No
            )
            if reply == QMessageBox.Yes:
                self.cancel_all()
                event.accept()
            else:
                event.ignore()
        else:
            if self.scanner and self.scanner.isRunning():
                self.scanner.cancel()
                self.scanner.wait()
            event.accept()

if __name__ == "__main__":
    app = QApplication(sys.argv)
    app.setStyle("Fusion")
    window = MainWindow()
    window.show()
    sys.exit(app.exec())