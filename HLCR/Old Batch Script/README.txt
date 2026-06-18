=============================================================
                  HLAE Clip Rendering Guide
=============================================================
NOTE: THIS CURRENTLY DOES NOT WORK WITH SPLIT STREAMS


This setup works with HLAE using the mirv_recordmovie_start command.
It assumes you have basic knowledge of HLAE.

-------------------------------------------------------------
PART 1: EXTRACT THE FILES
-------------------------------------------------------------

1. Extract the contents of this ZIP file to a dedicated folder.
   Example:
       D:\dod\tools\HLAE_Renderer\

2. Inside, you will find:
       - render_from_config.bat   (main render script)
       - render_config.ini        (configuration file)
       - ffmpeg.exe               (included for convenience)


-------------------------------------------------------------
PART 2: RECORDING CLIPS IN HLAE
-------------------------------------------------------------

To keep things organized and avoid overwriting clips,
set your mirv_movie_filename to match your demo name before recording.

Example:
       mirv_movie_filename s4-w2-axis; viewdemo s4-w2-axis

This will automatically create a folder:
       /half-life/s4-w2-axis/

All clips recorded from that demo will be saved inside this folder.


-------------------------------------------------------------
PART 3: PREPARING THE RENDER SCRIPT
-------------------------------------------------------------

1. Open each demo clip folder. Inside, you’ll find one or more
   “takeXXXX” folders. Each contains files like:

       \all\00000.bmp, 00001.bmp, ...
       fast-stairs-3k.wav

2. Preview a few BMPs to identify the clip, then rename the WAV file
   (one folder above the BMPs) to something descriptive, such as:
       middle-wallshot.wav

3. When finished renaming all WAV files, move all of your demo
   clip folders into a single master “clips” folder, for example:
       D:\steam\steamapps\common\Half-Life\clips\

4. Open the file “render_config.ini” in Notepad.

5. Set the following values inside it:

       ffmpeg_path=D:\dod\tools\ffmpeg\bin\ffmpeg.exe
       source_folder=D:\steam\steamapps\common\Half-Life\clips
       output_folder=D:\dod\Twist-Movie\clips
       fps=180

6. Save and close the INI file.

7. Double-click “render_from_config.bat”.
   You will see progress in the Command Prompt window as clips render.


-------------------------------------------------------------
AFTER THE SCRIPT HAS FINISHED
-------------------------------------------------------------

1. Verify your rendered clips.
   - Files are encoded as Apple ProRes 422 HQ.
   - They may appear choppy in regular media players (this is normal).
   - Import them into your video editor (Premiere, Resolve, etc.) to
     confirm playback.

2. Once you have verified all clips, delete the original BMP folders
   to save disk space. The rendered MOV files will remain in your
   chosen output folder.


-------------------------------------------------------------
TIPS
-------------------------------------------------------------

- Make sure the FPS value in render_config.ini matches your HLAE
  recording FPS. If not, the video will play too fast or too slow.

- Parentheses and spaces in filenames are supported.

- You can create multiple configuration files for different projects:
      render_config_twistmovie.ini
      render_config_exhibition.ini

  To use a different one, run:
      render_from_config.bat render_config_exhibition.ini


-------------------------------------------------------------
EXAMPLE FOLDER STRUCTURE
-------------------------------------------------------------

Half-Life\
 └── clips\
      ├── s4w2h2\
      │    ├── take0000\
      │    │    ├── all\
      │    │    │   ├── 00000.bmp
      │    │    │   ├── 00001.bmp
      │    │    │   └── ...
      │    │    └── middle-wallshot.wav
      │    └── take0001\
      │         └── ...
      └── s4w2h3\
           └── ...

=============================================================
                      END OF GUIDE
=============================================================
