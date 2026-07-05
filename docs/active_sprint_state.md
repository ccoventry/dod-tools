## Web AI State
- Goal: Inject director events into POV demos to enable UI bookmarking.
- Last Edited: `local/scripts/test_director_inject.rs`.
- Status: Script logic now correctly targets the 'Playback' directory entry, avoiding the Initialization Rule. Pending test compilation and execution.

## IDE AI State
- Immediate Task: Run `rustc local\scripts\test_director_inject.rs -o local\test_director_inject.exe`, execute against `analysis_target_pov.dem`, and verify UI population.
