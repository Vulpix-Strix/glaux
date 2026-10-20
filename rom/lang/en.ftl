# Glaux user interface text, English. Every other language falls back to
# this catalog for messages it does not translate.
#
# Text must use precomposed letters, because the system font has no
# combining marks. Keep API names, filenames, key names and command tokens
# unchanged when translating the surrounding prose.

## Workspace tabs

tab-code = Code
tab-map = Map
tab-music = Music
tab-sfx = SFX
tab-sprites = Sprites

## System and boot

boot-128-entries = 128 ENTRIES
boot-audio = AUDIO
boot-display-description = { $width }x{ $height } INDEXED
boot-ok = OK
boot-ok-2 = { " " }OK
boot-ready = READY.
boot-system-rom = SYSTEM ROM { $version }
boot-video = VIDEO
system-detect = Detect
system-did-not-run = { $action } did not run.
system-glaux-did-not-quit = Glaux did not quit.
system-graphics = graphics
system-identity = { $name } { $generation }  SYSTEM ROM { $rom-version }
system-nothing-to-edit-try-new-id-or-load-path = Nothing to edit. Try 'new <id>' or 'load <path>'.
system-paused = Paused { $identity }
system-reload-failed = RELOAD FAILED
system-resumed = Resumed { $identity }
system-running = Running { $identity }
system-running-with-unsaved-graphics-changes = Running { $identity } with unsaved graphics changes
system-save-failed = SAVE FAILED
system-saved-command-stopped = Saved { $names } before stopping. { $action } did not run.
system-saved-exit-stopped = Saved { $names } before stopping. Glaux did not quit.
system-spaces = Spaces
system-stopped = Stopped { $identity }
system-tabs = Tabs
system-type-resume-to-continue-or-stop-to-end = Type 'resume' to continue or 'stop' to end.
system-unloaded = Unloaded { $identity }
system-your-other-edits-are-still-in-memory = Your other edits are still in memory.

## Shell

shell-aliases = Aliases: { $value }
shell-cartridge = CARTRIDGE
shell-cartridge-paused-resume-or-stop-it-first = Cartridge paused: resume or stop it first
shell-change-the-current-directory = Change the current directory
shell-channel-must-be-0 = Channel must be 0-{ $value }
shell-clear-the-console = Clear the console
shell-close-the-quoted-text-with-another = Close the quoted text with another { $quote }
shell-create-a-new-cartridge = Create a new cartridge
shell-display = DISPLAY
shell-display-description = { $width }x{ $height } INDEXED, { $colors } COLORS
shell-doesn-t-take-any-arguments = { $value } doesn't take any arguments
shell-every-great-game-starts-somewhere = Every great game starts somewhere.
shell-frequency-must-be-1-hz = Frequency must be 1-{ $MAX-FREQUENCY } Hz
shell-gate-must-be-on-or-off-not = Gate must be on or off, not { $value }
shell-idle = IDLE
shell-list-a-directory = List a directory
shell-load-a-cartridge = Load a cartridge
shell-loaded = LOADED
shell-loaded-2 = Loaded { $value }
shell-machine = MACHINE
shell-missing = Missing { $name }
shell-missing-an-argument = Missing an argument
shell-missing-hz = Missing <hz>
shell-missing-id = Missing <id>
shell-no-cartridge-loaded = No cartridge loaded
shell-no-paused-cartridge = No paused cartridge
shell-none = NONE
shell-open-or-close-a-channel-gate = Open or close a channel gate
shell-options-name-version-author-lang-discard = Options: --name, --version, --author, --lang, --discard
shell-paths-with-spaces-need-quotes = Paths with spaces need quotes
shell-paused = PAUSED
shell-play-a-test-tone = Play a test tone
shell-ready-type-help-for-a-list-of-commands = Ready. Type 'help' for a list of commands.
shell-resume-the-paused-cartridge = Resume the paused cartridge
shell-rhai-experimental = RHAI SUPPORT IS EXPERIMENTAL. THE SCRIPTING API MAY CHANGE WITHOUT NOTICE.
shell-run-the-loaded-cartridge = Run the loaded cartridge
shell-runtime = RUNTIME
shell-set-a-channel-envelope = Set a channel envelope
shell-show-available-commands = Show available commands
shell-show-system-information = Show system information
shell-show-the-current-directory = Show the current directory
shell-show-the-system-rom-version = Show the System ROM version
shell-state = STATE
shell-stop-the-paused-cartridge = Stop the paused cartridge
shell-sustain-must-be-0 = Sustain must be 0-{ $MAX-SUSTAIN }
shell-system-rom = SYSTEM ROM
shell-the-channel-goes-before-off = The channel goes before off
shell-the-id-must-be-one-word-without-spaces = The id must be one word, without spaces
shell-this-one-starts-with-you = This one starts with you.
shell-tick-frequency = { $rate } HZ
shell-tick-rate = TICK RATE
shell-time-must-be-0-ms = Time must be 0-{ $MAX-ENVELOPE-MS } ms
shell-too-many-arguments = Too many arguments
shell-too-many-arguments-values-with-spaces-need-quotes = Too many arguments; values with spaces need quotes
shell-try = Try: { $command }
shell-try-run-to-see-your-first-creation = Try 'run' to see your first creation.
shell-unknown-command = Unknown command: { $name }
shell-unload-the-loaded-cartridge = Unload the loaded cartridge
shell-usage = Usage: { $usage }
shell-values-with-spaces-need-quotes = Values with spaces need quotes
shell-volume-must-be-0 = Volume must be 0-{ $MAX-VOLUME }
shell-warn = WARN: { $text }
shell-wave-must-be-square-triangle-saw-or-noise = Wave must be square, triangle, saw or noise
shell-your-new-cartridge-is-ready = Your new cartridge is ready!

## Workspace and dialogs

workspace-a-new-appeared-outside-glaux = A new { $asset } appeared outside Glaux.
workspace-already-exists = { $value } already exists.
workspace-and = { $value } and { $last }
workspace-arrange-sound-effects-into-songs = Arrange sound effects into songs.
workspace-asset-conflict-instructions = { $count ->
    [one] OVERWRITE DISK replaces the file with your edits. DISCARD & RELOAD throws away your unsaved edits to it and the undo steps involving it.
   *[other] OVERWRITE DISK replaces these files with your edits. DISCARD & RELOAD throws away your unsaved edits to them and the undo steps involving them.
    }
workspace-cancel = CANCEL
workspace-cannot-import = CANNOT IMPORT
workspace-cannot-open = Cannot open { $path }: { $err }
workspace-cannot-open-the-entry-script = Cannot open the entry script
workspace-cannot-read-files = CANNOT READ FILES
workspace-cartridge-storage-is-unavailable = Cartridge storage is unavailable.
workspace-changed-on-disk-reloaded-undo-steps-dropped = { $value } changed on disk: reloaded, undo steps dropped
workspace-changed-outside-glaux = { $path } changed outside Glaux.
workspace-close-unsaved-instructions = SAVE writes them to the file, then closes it. DISCARD closes it without saving.
workspace-created = Created { $path }
workspace-delete = DELETE
workspace-delete-already-missing = { $path } was already deleted outside Glaux. DELETE only closes its tab.
workspace-delete-external-warning = It changed outside Glaux; those changes will be lost.
workspace-delete-failed = DELETE FAILED
workspace-delete-from-the-cartridge = Delete { $path } from the cartridge?
workspace-delete-read-failed = Glaux could not read { $path } to check it for outside changes.
workspace-delete-script = DELETE SCRIPT
workspace-delete-unsaved-warning = Its unsaved changes will be lost.
workspace-deleted = Deleted { $path }
workspace-discard = DISCARD
workspace-discard-reload = DISCARD & RELOAD
workspace-draw-object-tiles-and-their-palettes = Draw object tiles and their palettes.
workspace-edit-the-cartridge-s-luau-and-rhai-scripts = Edit the cartridge's Luau and Rhai scripts.
workspace-edit-write-failed = Glaux could not change { $path }. Check the cartridge folder's permissions, and that it is a regular file.
workspace-esc-shell-ctrl-tab-switch-tab = Esc shell   Ctrl+Tab switch tab
workspace-file-conflict = FILE CONFLICT
workspace-glaux-could-not-read-to-check-it-for-outside-changes-check-its-pe-2 = Glaux could not read { $path } to check it for outside changes. Check its permissions, then save again.
workspace-glaux-could-not-write-check-the-cartridge-folder-s-permissions-an-2 = Glaux could not write { $path }. Check the cartridge folder's permissions and free space, then save again.
workspace-has-unsaved-changes = { $path } has unsaved changes.
workspace-is-not-a-regular-file-move-it-aside-then-save-again-2 = { $path } is not a regular file. Move it aside, then save again.
workspace-is-read-only-make-it-writable-then-save-again-2 = { $path } is read-only. Make it writable, then save again.
workspace-is-read-only-make-it-writable-then-try-again = { $path } is read-only. Make it writable, then try again.
workspace-lay-out-tiles-into-maps = Lay out tiles into maps.
workspace-no-file-chosen = No file chosen
workspace-not-available-yet = Not available yet.
workspace-nothing-to-save = Nothing to save
workspace-nothing-was-changed = Nothing was changed.
workspace-nothing-was-deleted = Nothing was deleted.
workspace-opening = Opening { $value }
workspace-overwrite-disk = OVERWRITE DISK
workspace-overwrite-disk-replaces-the-file-with-your-edits = OVERWRITE DISK replaces the file with your edits.
workspace-overwrite-disk-writes-your-edits-to-it-again = OVERWRITE DISK writes your edits to it again.
workspace-reload-failed = RELOAD FAILED
workspace-reloaded-from-disk = Reloaded { $value } from disk
workspace-renamed-to = Renamed to { $to }
workspace-save = SAVE
workspace-save-all = SAVE ALL
workspace-save-failed = SAVE FAILED
workspace-save-missing-folder = The folder holding { $path } is gone. Recreate it, then save again.
workspace-saved = Saved { $value }
workspace-saved-before-stopping = Saved { $value } before stopping.
workspace-saved-bytes = Saved { $value } ({ $value-2 } bytes)
workspace-shape-sound-effects-for-the-psg = Shape sound effects for the PSG.
workspace-that-file-is-no-longer-open = That file is no longer open
workspace-the-cartridge-s-sprites-are-not-loaded = The cartridge's sprites are not loaded.
workspace-the-file-cannot-be-recovered = The file cannot be recovered.
workspace-unsaved-command = { $count ->
    [one] SAVE ALL saves it, then runs { $action }. DISCARD throws it away and runs { $action } anyway.
   *[other] SAVE ALL saves them, then runs { $action }. DISCARD throws them away and runs { $action } anyway.
    }
workspace-unsaved-edits-kept = Your unsaved edits are still in memory.
workspace-unsaved-exit = { $count ->
    [one] SAVE ALL saves it, then quits Glaux. DISCARD throws it away and quits Glaux anyway.
   *[other] SAVE ALL saves them, then quits Glaux. DISCARD throws them away and quits Glaux anyway.
    }
workspace-unsaved-names = { $names } { $count ->
    [one] has
   *[other] have
    } unsaved changes.
workspace-unsaved-title = UNSAVED CHANGES
workspace-up-down-pick-enter-run-esc-close = Up/Down: pick  Enter: run  Esc: close
workspace-was-deleted-outside-glaux = { $path } was deleted outside Glaux.
workspace-your-edits-are-still-in-memory = Your edits are still in memory.

## Code editor

code-all = All
code-and-more-places-not-searched = ...and { $value } more places not searched
code-cannot-list-scripts = Cannot list scripts: { $error }
code-cannot-read-folder = Cannot read folder { $value }
code-caret-position = Ln { $line }  Col { $column }
code-close-tab = Close tab
code-completion-close = { $key }: close
code-completion-pick = { $key }: pick
code-copy = Copy
code-creates = Creates { $path }
code-ctrl-s-save-ctrl-f-find-ctrl-n-new-esc-shell = Ctrl+S: save  Ctrl+F: find  Ctrl+N: new  Esc: shell
code-cut = Cut
code-delete-script = Delete script...
code-done = Done
code-editor-settings = EDITOR SETTINGS
code-enter-go-esc-cancel = Enter: go  Esc: cancel
code-enter-next-up-previous-tab-field-esc-close = Enter: next  Up: previous  Tab: field  Esc: close
code-esc-close = Esc: close
code-find = Find...
code-find-whole-word-button = W
code-function-declared = Function declared on line { $line }.
code-go-to-line = Go to line...
code-go-to-line-2 = GO TO LINE
code-indent-size = Indent size
code-indent-with = Indent with
code-invalid-script-name = Not a valid .{ $extension } file name.
code-ligatures = Ligatures
code-line-number-1 = Line number (1-{ $lines }):
code-line-out-of-range = Enter a line from 1 to { $lines }.
code-listing-stopped-after-files = Listing stopped after { $limit } files
code-local-variable-declared = Local variable declared on line { $line }.
code-moves-it-to = Moves it to { $path }
code-new-file = New file...
code-new-file-2 = NEW FILE
code-new-file-ctrl-n = New file    Ctrl+N
code-new-script-description = New .{ $extension } script in { $directory }
code-new-script-extension-hint = Leave out .{ $extension }; it is added.
code-new-script-language-hint = This cartridge is { $language }; .{ $extension } is added.
code-new-script-name-hint = Type a name; .{ $extension } is added.
code-no-documentation-for = No documentation for { $value }
code-no-file-matches = No file matches.
code-no-file-open = No file open.
code-no-luau-or-rhai-files-in-this-cartridge = No .luau or .rhai files in this cartridge.
code-open-file = Open file...
code-open-file-2 = OPEN FILE
code-open-file-ctrl-p = Open file   Ctrl+P
code-open-file-tag = open
code-parameter-declared = Parameter declared on line { $line }.
code-paste = Paste
code-picker-file-count = { $matches } of { $total } files
code-redo = Redo
code-rename-description = Move { $from } within { $directory }
code-rename-script = Rename script...
code-rename-script-2 = RENAME SCRIPT
code-replace = Replace...
code-replace-2 = Replace
code-replaced = Replaced { $value }
code-select-all = Select all
code-show-line-numbers = Show line numbers
code-signature-declaration = Declared on line { $line }.
code-skipped-link-to-a-listed-folder = Skipped link to a listed folder: { $value }
code-tab-size = Tab size
code-the-entry-script-could-not-be-opened = The entry script could not be opened:
code-too-deep-not-searched = Too deep, not searched: { $value }
code-type-a-name-for-the-script = Type a name for the script.
code-type-a-new-path = Type a new path.
code-type-filter-enter-open-esc-close = Type: filter  Enter: open  Esc: close
code-type-line-enter-go-esc-close = Type: line  Enter: go  Esc: close
code-type-path-enter-create-esc-close = Type: path  Enter: create  Esc: close
code-type-path-enter-move-esc-close = Type: path  Enter: move  Esc: close
code-undo = Undo
code-up-down-pick-enter-show-esc-close = Up/Down: pick  Enter: show  Esc: close
code-up-down-pick-enter-toggle-esc-close = Up/Down: pick  Enter: toggle  Esc: close
code-up-down-pick-tab-accept-esc-close = Up/Down: pick  Tab: accept  Esc: close
code-use-names-joined-by-without-or = Use names joined by /, without .., \ or :
code-word-wrap = Word wrap

## Sprite editor

sprites-a-hide-palette-details = A: hide palette details
sprites-a-show-palette-details = A: show palette details
sprites-advanced = Advanced
sprites-alpha-only = Alpha only
sprites-apply = APPLY
sprites-apply-to-all = APPLY TO ALL
sprites-arrows-choose-enter-apply-esc-cancel = Arrows: choose  Enter: apply  Esc: cancel
sprites-can-t-show-this-cartridge-s-sprites = Can't show this cartridge's sprites:
sprites-cannot-import = Cannot import: { $message }
sprites-click-choose-this-tile-s-palette = Click: choose this tile's palette
sprites-click-choose-this-tile-s-palette-from-obp = Click: choose this tile's palette, from OBP { $bank }
sprites-click-fill-the-area = Click: fill the area
sprites-click-pick-its-color = Click: pick its color
sprites-click-replace-all-sprites-with-a-png-sheet = Click: replace all sprites with a PNG sheet
sprites-click-select-color-double-click-edit = Click: select color { $local }  Double-click: edit
sprites-click-select-tile = Click: select tile { $tile } ({ $column },{ $row })
sprites-click-select-transparent-color-0 = Click: select transparent color 0
sprites-code-that-draws-tiles-by-number-will-show-the-new-ones = Code that draws tiles by number will show the new ones.
sprites-code-that-draws-tiles-or-colors-by-number-will-show-the-new-ones = Code that draws tiles, or colors { $range }, by number will show the new ones.
sprites-color-0-is-transparent = Color 0 is transparent
sprites-color-already-matches = ${ $hex } already has this color.
sprites-color-background-code = Game background; code may use it too.
sprites-color-background-code-changes = Background and code using 0 change too.
sprites-color-code-index-changes = Code drawing { $index } changes too.
sprites-color-code-master-changes = Code drawing color { $master } changes too.
sprites-color-code-number-changes = Code using that color number changes.
sprites-color-game-code = Game code may use it too.
sprites-color-heading = Palette { $bank }, color { $local }
sprites-color-heading-advanced = OBP{ $bank } L{ $local }  Master ${ $master-hex } ({ $master })
sprites-color-other-palettes-unchanged = Other palettes keep the old color.
sprites-color-scope-all-master = All: master ${ $hex }, { $entries ->
    [one] { $entries } more entry
   *[other] { $entries } more entries
    }
sprites-color-scope-all-palettes = All { $banks } palettes, { $tiles ->
    [0] no other tiles
    [one] { $tiles } other tile
   *[other] { $tiles } other tiles
    }
sprites-color-scope-only-master = OBP{ $bank } only: master ${ $hex } ({ $index })
sprites-color-scope-only-palette = Only Palette { $bank }, { $tiles ->
    [0] no other tiles
    [one] { $tiles } other tile
   *[other] { $tiles } other tiles
    }
sprites-color-shared-master = ! Master ${ $hex }: { $entries ->
    [one] { $entries } more entry
   *[other] { $entries } more entries
    }, { $banks ->
    [0] this bank
    [one] { $banks } bank
   *[other] { $banks } banks
    }
sprites-color-shared-palettes = { $banks ->
    [0] ! Shared with { $entries ->
        [one] { $entries } other color
       *[other] { $entries } other colors
        } of
    [one] ! Shared: also in { $banks } other palette
   *[other] ! Shared: also in { $banks } other palettes
    }
sprites-color-shared-tiles = { "  " }{ $banks ->
    [0] this palette; { $tiles ->
        [0] no other tiles
        [one] { $tiles } other tile
       *[other] { $tiles } other tiles
        }.
   *[other] ({ $entries ->
        [one] { $entries } palette color
       *[other] { $entries } palette colors
        }), { $tiles ->
        [0] no other tiles
        [one] { $tiles } other tile
       *[other] { $tiles } other tiles
        }.
    }
sprites-color-tiles-code = { $tiles ->
    [one] { $tiles } other tile
   *[other] { $tiles } other tiles
    }; { $master ->
    [0] code, background too.
   *[other] code using { $master } too.
    }
sprites-color-unshared-master = Master ${ $hex } ({ $master }) is not shared.
sprites-color-unshared-tiles = { $tiles ->
    [0] Changes it in every tile that uses it.
    [one] Also shows in { $tiles } other tile.
   *[other] Also shows in { $tiles } other tiles.
    }
sprites-ctrl-s-write-the-changed-files = Ctrl+S: write the changed files
sprites-current = Current: { $value }
sprites-drag-draw = Drag: draw
sprites-drag-erase = Drag: erase
sprites-drag-pick-tab-next-enter-apply-esc-cancel = Drag: pick  Tab: next  Enter: apply  Esc: cancel
sprites-edit-color = EDIT COLOR
sprites-eraser = Eraser
sprites-eyedropper = Eyedropper
sprites-fill = Fill
sprites-fix-the-file-then-reopen-the-workspace = Fix the file, then reopen the workspace.
sprites-g-show-or-hide-the-pixel-grid = G: show or hide the pixel grid
sprites-grid = Grid
sprites-hue = HUE
sprites-image-dimensions = { $width }x{ $height } px
sprites-import = Import
sprites-import-2 = IMPORT
sprites-import-color-count = { $colors } colors, { $new } new
sprites-import-groups = { $count ->
    [0] No sprite groups
    [one] 1 sprite group
   *[other] { $count } sprite groups
    }
sprites-import-image-summary = { $width }x{ $height } px, { $tiles } tiles
sprites-import-palette-count = { $used } of { $total } palettes
sprites-import-sprites = IMPORT SPRITES
sprites-internal-error = internal error: { $err }
sprites-local = Local
sprites-master = Master
sprites-modified = MODIFIED
sprites-new = NEW
sprites-no-unsaved-edits = No unsaved edits
sprites-nothing-is-written-until-save-ctrl-z-undoes-it = Nothing is written until Save. Ctrl+Z undoes it.
sprites-nothing-to-redo = Nothing to redo
sprites-nothing-to-undo = Nothing to undo
sprites-old = OLD
sprites-palette-2 = Palette
sprites-palette-button = { $advanced ->
    [yes] OBP{ $bank }...
   *[no] Palette { $bank }...
    }
sprites-palette-name = Palette { $bank }
sprites-palettes-and-master-colors-change = { $value } palettes and master colors { $range } change.
sprites-palettes-change = { $value } palettes change.
sprites-pencil = Pencil
sprites-replaces-every-sprite-tiles-change = Replaces every sprite: { $value } tiles change.
sprites-replaces-every-sprite-tiles-change-drawn-ones-erased = Replaces every sprite: { $value } tiles change, { $erased } drawn ones erased.
sprites-save = Save
sprites-select-palette = SELECT PALETTE
sprites-selected-color = Selected: Local { $local } -> Master { $master } (${ $master-hex }) -> #{ $r }{ $g }{ $b }
sprites-selected-local-transparent = Selected: Local { $local } -> transparent
sprites-status = Tile { $tile }{ $advanced ->
    [yes] { " " }({ $column },{ $row }){ $has-bank ->
        [yes] { " | OBP " }{ $bank } | 4 bpp
       *[no] { "" }
        }
   *[no] { "" }
    }{ $has-pixel ->
    [yes] { " | X:" }{ $x } Y:{ $y } | Color:{ $color }
   *[no] { "" }
    }
sprites-stopped-waiting-for-the-file-dialog = Stopped waiting for the file dialog
sprites-t-transparency-enter-import-esc-cancel = T: transparency  Enter: import  Esc: cancel
sprites-the-sheet-is-at-its-largest = The sheet is at its largest
sprites-the-sheet-is-at-its-smallest = The sheet is at its smallest
sprites-the-sheet-matches-the-current-sprites-nothing-changes = The sheet matches the current sprites: nothing changes.
sprites-tile = Tile { $tile }
sprites-tile-coordinates = Tile { $tile } ({ $column },{ $row })
sprites-tool-hint = { $key }: { $tool ->
    [pencil] Pencil
    [eraser] Eraser
    [eyedropper] Eyedropper
   *[fill] Fill
    }
sprites-top-left-color = Top-left color
sprites-transparent = Transparent:
sprites-waiting-for-the-file-dialog-esc-stop-waiting = Waiting for the file dialog. Esc: stop waiting
sprites-wheel-scroll-arrows-select = Wheel: scroll  Arrows: select
sprites-wheel-scroll-shift-wheel-pan = Wheel: scroll  Shift+wheel: pan
sprites-zoom-the-sheet-in = +: zoom the sheet in
sprites-zoom-the-sheet-out = -: zoom the sheet out

## Documentation browser

docs-all = All
docs-alt-left-back-alt-right-fwd-ctrl-b-nav = Alt+Left: back  Alt+Right: fwd  Ctrl+B: nav
docs-api = API
docs-blocked-link = Blocked link: { $scheme }
docs-broken-link = Broken link: { $target }
docs-button = Docs
docs-button-hint = Docs (F1)
docs-caution = Caution
docs-docs-tab-limit-reached-close-a-tab-first = Docs tab limit reached ({ $MAX-TABS }). Close a tab first.
docs-guides = Guides
docs-important = Important
docs-nav = Nav
docs-no-page = No page { $value }
docs-no-page-matches = No page matches.
docs-note = Note
docs-on-this-page = On this page
docs-related = Related
docs-search = Search
docs-search-docs = SEARCH DOCS
docs-search-page-count = { $count ->
    [one] { $count } page
   *[other] { $count } pages
    }
docs-tip = Tip
docs-type-query-tab-filter-enter-open-esc-close = Type: query  Tab: filter  Enter: open  Esc: close
docs-up-down-pick-enter-show-esc-close = Up/Down: pick  Enter: show  Esc: close
docs-warning = Warning

## Desktop dialogs

desktop-file-dialog-already-open = A file dialog is already open.
desktop-import-spritesheet = Import spritesheet
desktop-png-image = PNG image
desktop-the-file-dialog-stopped-unexpectedly = The file dialog stopped unexpectedly.

## Visible errors

error-cartridge-asset-not-a-file = { $asset } is not a file
error-cartridge-asset-read-only = { $asset } is read-only
error-cartridge-empty-name = cartridge.ron: cartridge.name is empty
error-cartridge-entry-not-a-file = Entry is not a file: { $path }
error-cartridge-entry-not-found = Entry not found: { $path }
error-cartridge-entry-not-luau = cartridge.ron: runtime.entry must be a .luau file
error-cartridge-entry-not-rhai = cartridge.ron: runtime.entry must be a .rhai file
error-cartridge-entry-unreadable = Cannot read entry: { $path }
error-cartridge-invalid-entry-path = cartridge.ron: runtime.entry must be a relative path
error-cartridge-invalid-min-glaux = cartridge.ron: compatibility.min_glaux is not semver
error-cartridge-invalid-version = cartridge.ron: cartridge.version is not semver
error-cartridge-legacy-manifest = cart.json is no longer supported; use cartridge.ron
error-cartridge-malformed = cartridge.ron line { $line }: { $reason }
error-cartridge-manifest-not-a-file = cartridge.ron is not a file
error-cartridge-manifest-unreadable = Cannot read cartridge.ron
error-cartridge-missing-field = cartridge.ron: missing { $field }
error-cartridge-missing-manifest = No cartridge.ron in { $directory }
error-cartridge-requires-glaux = Cartridge requires Glaux { $required } (this is { $running })
error-cartridge-unknown-field = cartridge.ron: unknown field { $field }
error-cartridge-unsupported-format = cartridge.ron: unsupported format { $format }
error-cartridge-unsupported-graphics = cartridge.ron: unsupported graphics profile { $profile }
error-cartridge-unsupported-language = cartridge.ron: unsupported language { $language }
error-cartridge-unsupported-model = cartridge.ron: unsupported hardware model { $model }
error-create-empty-name = The name must not be empty
error-create-exists = { $path } already exists
error-create-invalid-author = The author must be 1-{ $max-text-len } characters, without control characters
error-create-invalid-id = Invalid cartridge id '{ $id }': use lowercase letters, digits and dashes, like my-game
error-create-invalid-name = The name must not contain control characters
error-create-invalid-version = Invalid version '{ $version }': use semver, like 1.2.0
error-create-name-too-long = The name must be at most { $max-text-len } characters
error-create-unsupported-language = Unsupported language '{ $language }': use luau or rhai
error-document-invalid-utf8 = { $path } line { $line }: not valid UTF-8 text
error-document-not-found = No such file: { $path }
error-fit-duplicate-group = sprite group "{ $name }" is defined more than once
error-fit-empty-group = sprite group "{ $name }" covers no tiles
error-fit-empty-group-name = a sprite group has an empty name
error-fit-group-outside-sheet = sprite group "{ $name }" reaches outside the sheet
error-fit-group-too-colorful = sprite group "{ $name }" uses { $colors } colors; one sprite palette holds { $bank-colors }. Split the group or reduce its colors
error-fit-groups-overlap = sprite groups "{ $name }" and "{ $other-name }" overlap
error-fit-invalid-dimensions = sprite sheet dimensions must be divisible by 8, got { $width }x{ $height }
error-fit-round-trip = internal error: pixel ({ $x }, { $y }) would not keep its color; nothing was written
error-fit-semi-transparent = { $count } pixels are partly transparent, the first at ({ $x }, { $y }) with alpha { $alpha }; sprite pixels must be fully opaque or fully transparent
error-fit-tile-too-colorful = tile ({ $tile-x }, { $tile-y }), pixels ({ $x }, { $y }) to ({ $end-x }, { $end-y }), uses { $colors } colors; an 8x8 sprite tile can use at most { $bank-colors } plus transparency
error-fit-too-large = sprite sheet is { $width }x{ $height }; the G1 object-tile grid holds at most { $OBJ-SHEET-MAX-WIDTH }x{ $OBJ-SHEET-MAX-HEIGHT }
error-fit-too-many-colors = sprite sheet uses { $colors } colors; G1 shows at most { $max-sheet-colors } sprite colors ({ $OBJ-PALETTE-BANKS } sprite palettes of { $bank-colors } colors)
error-fit-too-many-palettes = the sheet needs more than { $OBJ-PALETTE-BANKS } sprite palettes: { $unit } ({ $colors } colors) does not fit next to the colors already placed (palettes hold { $bank-sizes } colors). Reduce the colors of the most colorful sprites, or share more colors between them
error-fs-already-exists = Already exists: { $path }
error-fs-not-a-directory = Not a directory: { $path }
error-fs-not-a-file = Not a file: { $path }
error-fs-not-found = No such file or directory: { $path }
error-fs-read-only = Read-only: { $path }
error-fs-unavailable = Cartridge storage unavailable
error-fs-unreadable = Cannot read { $path }
error-fs-unwritable = Cannot write { $path }
error-module-name-empty = module name is empty
error-module-name-empty-segment = module name has an empty segment
error-module-name-has-extension = module names take no extension; use "{ $stem }", not "{ $stem }.{ $extension }"
error-module-name-invalid-character = { $character } is not allowed; use lowercase letters, digits, '_' and '.'
error-module-name-leading-digit = segment '{ $segment }' starts with a digit
error-move-script-changed = { $path } changed on disk outside Glaux. Close and reopen it before renaming.
error-move-script-missing = { $path } no longer exists on disk.
error-obj-attributes-reserved-bits = object tile { $tile } attribute { $value } sets reserved bits; only bits 0-2 are defined
error-obj-attributes-too-large = object tile attribute data is too large: got { $length } bytes, maximum is { $OBJ-TILE-ATTRIBUTES-BYTES }
error-obj-encoding-palette-index = palette index { $index } at pixel ({ $x }, { $y }), tile ({ $tile-x }, { $tile-y }), is outside 0..=15
error-obj-encoding-pixel-count = expected { $expected } pixels for a { $width }x{ $height } sprite sheet, got { $actual }
error-obj-encoding-sheet-too-tall = sprite sheet is { $height } pixels tall; the G1 object-tile grid is at most { $maximum }
error-obj-encoding-sheet-too-wide = sprite sheet is { $width } pixels wide; the G1 object-tile grid is at most { $maximum }
error-obj-palettes-length = object palette data must be exactly { $OBJ-PALETTES-BYTES } bytes, got { $length }
error-obj-palettes-reserved-entry = object palette bank { $bank } entry 0 must be 0, got { $value }; local index 0 is transparent
error-obj-tiles-misaligned = object tile data length must be a multiple of { $OBJ-TILE-BYTES } bytes, got { $length }
error-obj-tiles-too-large = object tile data is too large: got { $length } bytes, maximum is { $OBJ-TILE-BANK-BYTES }
error-option-missing-value = Option --{ $option } needs a value
error-option-repeated = Option --{ $option } given more than once
error-option-unknown = Unknown option: { $option }
error-palette-bytes = palette data must be exactly { $PALETTE-RGB888-BYTES } bytes, got { $length }
error-parse-unterminated-quote = Unterminated { $quote } quote
error-preview-palette = the object palettes reference { $needed } master colors, but only { $available } palette entries are free after the { $reserved } reserved ones
error-rename-language = { $path } is not a script of the same language
error-rename-not-open = The script is not open
error-rename-path-taken = { $path } is already open in another tab
error-script-limit = { $phase ->
    [entry] Execution limit exceeded
   *[callback] Execution limit exceeded in { $callback }
    }
error-script-module-limit = { $phase ->
    [entry] Execution limit exceeded in { $path }
   *[callback] Execution limit exceeded in { $callback }: { $path }
    }
error-script-require = { $kind ->
    [invalid] Invalid module name
    [missing] Module not found
    [unreadable] Cannot read module
   *[cycle] Cyclic require
    }{ $phase ->
    [entry] { "" }
   *[callback] { " in " }{ $callback }
    }: { $detail }{ $has-at ->
    [yes] { " (" }{ $at })
   *[no] { "" }
    }
error-script-runtime = { $phase ->
    [entry] Runtime error: { $detail }
   *[callback] Runtime error in { $callback }: { $detail }
    }
error-script-syntax = { $phase ->
    [entry] Syntax error: { $detail }
   *[callback] Syntax error in { $callback }: { $detail }
    }
error-source-edit-changed = { $path } changed on disk, so it was not deleted
error-source-edit-entry = { $path } is the entry script of this cartridge
error-source-edit-language = { $from } cannot become { $to }: a script keeps its language
error-source-path-invalid = Invalid path: { $path }
error-source-path-unsupported-extension = Not a Luau or Rhai file: { $path }
error-video-objattributes = Invalid objattr.bin: { $detail }
error-video-objpalettes = Invalid obp.bin: { $detail }
error-video-objtiles = Invalid sprites.bin: { $detail }
error-video-palette = Invalid palette.bin: { $detail }
