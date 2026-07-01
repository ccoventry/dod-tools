# Project Overview

## Purpose
`dod-tools` is a high-performance utility suite for parsing, analyzing, and manipulating Half-Life/Day of Defeat 1.3 demo files (`.dem`). It includes a rich graphical dashboard for match analytics and drives the Half-Life Clip Renderer (HLCR) to automate movie-making captures.

## Target Users
Competitive Day of Defeat players tracking analytics, and video editors generating automated frag movies.

## In Scope
- Low-level binary protocol parsing (`nom`/`dem`).
- Aggregate analytics (scoreboards, killstreaks).
- Automated external process management for Half-Life Advanced Effects (HLAE) cinematic recording.

## Out of Scope
- Direct video rendering or post-processing (handled externally via Vegas Pro / FFmpeg).
- Modern Source/Source 2 (CS:GO/CS2) engine support.
