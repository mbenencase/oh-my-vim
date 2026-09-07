# Changelog

Every entry below is generated on merge to `main` from the [Conventional
Commits](https://www.conventionalcommits.org/) in that release's range --
see `.github/workflows/release.yml`. Don't hand-edit past sections; fix the
commit convention instead.

<!-- releases -->

## v0.4.0

### Features

- draw the Markdown render pane beside its source window ([`a4594de`](https://github.com/mbenencase/oh-my-vim/commit/a4594de2e6fe3f268e8513602b7f15c684312cbc))

### Bug fixes

- lock the render pane's read-only invariants with tests ([`80ec0f4`](https://github.com/mbenencase/oh-my-vim/commit/80ec0f4d20a8259239ebbacd0890b825d6ed6ea6))
- keep the markdown render pane consistent across window-tree actions ([`952a516`](https://github.com/mbenencase/oh-my-vim/commit/952a516025fb990d430d4a9b73e679ca70d063b6))
- keep the markdown render pane live across edits and buffer switches ([`e558b27`](https://github.com/mbenencase/oh-my-vim/commit/e558b2778f5e7b2966c5df10f132181295cd67ae))

### Other changes

- correct the render pane's entry in CLAUDE.md ([`36f1357`](https://github.com/mbenencase/oh-my-vim/commit/36f13577b79fb1e8a3079c48ef002b1567f8a7bb))
- raise test-count ratchet from 83 to 127 and document Markdown render pane ([`771648b`](https://github.com/mbenencase/oh-my-vim/commit/771648b99106dea69aa4bc249c7603c50e723804))

**Full diff**: https://github.com/mbenencase/oh-my-vim/compare/v0.3.0...v0.4.0

## v0.3.0

### Features

- open a shell in a terminal panel with <C-j> ([`e7e5e0e`](https://github.com/mbenencase/oh-my-vim/commit/e7e5e0e4db5f92336723461c03a303f51174e393))
- split windows with :vsp and :hsp ([`f06df4c`](https://github.com/mbenencase/oh-my-vim/commit/f06df4cc1507de33fa03faddb8b7270284cf07ac))

**Full diff**: https://github.com/mbenencase/oh-my-vim/compare/v0.2.0...v0.3.0

## v0.2.0

### Features

- **find-and-replace-mechanism**: implement find and replace window ([`0bc100e`](https://github.com/mbenencase/oh-my-vim/commit/0bc100e70219839fb272b2ea1e18c1930dfc58fa))