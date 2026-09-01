# Test fixtures

This folder is where `dem-patch`'s ignored tests (`src/lib.rs`) expect a small demo
fixture — `demotest.dem` — to live. It's tracked in git (unlike `/demos/`, which is
gitignored scratch space for real-world debugging demos), so anything committed here
should be genuinely tiny.

Currently empty: `demotest.dem` hasn't been added yet — see
[issue #16](https://github.com/ccoventry/dod-tools/issues/16). Trimming a real demo
down to a minimal, still-valid fixture has turned out to be trickier than it sounds;
that's tracked separately, not solved by this folder existing.

Once a fixture is added, un-ignore the four tests in `src/lib.rs` currently marked
`#[ignore = "needs ./src/tests/demotest.dem, a local fixture not committed to the repo"]`.

Files this folder's tests are expected to produce as output (`demotest_out.dem`,
`demo2test.dem`) are gitignored — only commit the source fixture itself.
