# VHS tape: mdcat + mdless demo.
#
# Prep:
#   cargo build --release --bin mdcat --bin mdless
#
# Record:
#   vhs tapes/demo.tape
#
# Output: tapes/demo.gif

Output tapes/demo.gif

# Window chrome.
Set Shell "zsh"
Set FontFamily "JetBrains Mono"
Set FontSize 18
Set Width 1400
Set Height 820
Set Theme "Dracula"
Set Padding 28
Set Margin 28
Set MarginFill "#16161e"
Set BorderRadius 12
Set WindowBar Colorful
Set WindowBarSize 36
Set CursorBlink false
Set TypingSpeed 18ms
Set PlaybackSpeed 1.25
Set LoopOffset 1
# Lower frame rate for a smaller GIF; 15 fps is plenty for a text demo.
Set Framerate 15

# Stage: release binaries on PATH, custom prompt, clear.
Hide
Type "export PATH=$PWD/target/release:$PATH"
Enter
Type "export PS1='%F{magenta}➜%f %F{cyan}mdcat%f %% '"
Enter
Type "clear"
Enter
Sleep 200ms
Show

# Scene 1 — mdcat renders the showreel.
Type "mdcat sample/demo.md"
Sleep 300ms
Enter
Sleep 3000ms

Hide
Type "clear"
Enter
Show

# Scene 2 — mdless interactive viewer.
Type "mdless --search '\\d+' --regex sample/stress.md"
Sleep 250ms
Enter
Sleep 1000ms

# Cycle matches.
Type "nn"
Sleep 700ms

# Heading hops.
Escape
Sleep 200ms
Type "]]]]"
Sleep 700ms

# TOC modal, pick, jump.
Type "T"
Sleep 900ms
Type "jjj"
Sleep 500ms
Enter
Sleep 1000ms

# Quit.
Type "q"
Sleep 300ms
