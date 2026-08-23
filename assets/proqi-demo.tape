# Reproducible README recording of the real release binary.
Output assets/proqi-demo.gif

Require cargo

Set Shell "zsh"
Set FontFamily "Menlo"
Set FontSize 22
Set Width 1200
Set Height 720
Set TypingSpeed 25ms
Set PlaybackSpeed 1
Set LoopOffset 10%
Set Theme { "name": "Proqi", "black": "#0d1117", "red": "#ff7b72", "green": "#238636", "yellow": "#d29922", "blue": "#58a6ff", "magenta": "#bc8cff", "cyan": "#39c5cf", "white": "#c9d1d9", "brightBlack": "#484f58", "brightRed": "#ffa198", "brightGreen": "#3fb950", "brightYellow": "#e3b341", "brightBlue": "#79c0ff", "brightMagenta": "#d2a8ff", "brightCyan": "#56d4dd", "brightWhite": "#f0f6fc", "background": "#0d1117", "foreground": "#c9d1d9", "selection": "#264f36", "cursor": "#3fb950" }

Hide
Type "export PS1='proqi-demo% '"
Enter
Wait /proqi-demo/
Type "cargo build --release --locked >/dev/null 2>&1"
Enter
Wait /proqi-demo/
Type "./scripts/readme-demo.sh fresh"
Enter
Show

Wait+Screen /Unicode and exact whitespace/
Sleep 1s
Escape
Sleep 600ms
Down
Sleep 600ms
Up
Sleep 600ms
Space
Sleep 1s
Type "?"
Sleep 2s
Escape
Type "q"

Hide
Sleep 1s
Type "./scripts/readme-demo.sh resume"
Enter
Show
Wait+Screen /SQLite recovery/
Sleep 2s
Type "q"

Hide
Sleep 1s
Type "./scripts/readme-demo.sh clean"
Enter
Wait /proqi-demo/
Show
Sleep 1s
