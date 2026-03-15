# sfhtml demo recording — requires VHS (https://github.com/charmbracelet/vhs)
# Install: go install github.com/charmbracelet/vhs@latest
# Run:     vhs docs/demo.tape

Output docs/images/demo.gif

Set Shell "bash"
Set FontSize 14
Set Width 1000
Set Height 600
Set Theme "Dracula"
Set TypingSpeed 40ms

Type "# 1. Scan workspace for HTML files"
Enter
Sleep 500ms

Type "sfhtml scan . --recursive --json | head -20"
Enter
Sleep 2s

Type "# 2. Read the AI-SKILL-HEADER"
Enter
Sleep 500ms

Type "sfhtml header tests/fixtures/survey_app.html"
Enter
Sleep 2s

Type "# 3. Locate a code anchor"
Enter
Sleep 500ms

Type 'sfhtml locate tests/fixtures/survey_app.html "initApp" --context 5'
Enter
Sleep 2s

Type "# 4. Apply a diff safely"
Enter
Sleep 500ms

Type "sfhtml apply app.html --diff patch.diff --dry-run --json"
Enter
Sleep 2s

Type "# 5. Validate after editing"
Enter
Sleep 500ms

Type "sfhtml validate tests/fixtures/survey_app.html"
Enter
Sleep 2s

Type "# 6. Browser interaction"
Enter
Sleep 500ms

Type "sfhtml debug start tests/fixtures/survey_app.html"
Enter
Sleep 1.5s

Type "sfhtml page screenshot --output shot.png"
Enter
Sleep 1.5s

Type "sfhtml page console --json"
Enter
Sleep 1.5s

Type "sfhtml debug stop"
Enter
Sleep 1s
