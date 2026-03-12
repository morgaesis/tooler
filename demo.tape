# Tooler Demo - Terminal recording for VHS (https://github.com/charmbracelet/vhs)
# Run: vhs demo.tape

Output assets/demo.gif
Output assets/demo.svg

Set FontSize 14
Set Width 1100
Set Height 500
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms
Set Padding 20

# GitHub tool run with pinned version
Type "tooler run nektos/act@v0.2.79 --version"
Enter
Sleep 2s

# Pin a tool version
Type "tooler pin nektos/act@v0.2.79"
Enter
Sleep 1.5s

# Complex GitHub tag (tags with slashes)
Type "tooler run infisical/infisical@infisical-cli/v0.41.90 --version"
Enter
Sleep 2s

# Direct URL install
Type "tooler run https://dl.k8s.io/release/v1.31.0/bin/linux/arm64/kubectl version --client"
Enter
Sleep 2s

# List installed tools
Type "tooler list"
Enter
Sleep 3s

# Pull and shim a tool
Type "tooler pull cli/cli"
Enter
Sleep 2s

Type "gh --version"
Enter
Sleep 2s
