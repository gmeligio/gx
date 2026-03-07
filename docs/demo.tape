Output demo.gif

Set Theme "Catppuccin Mocha"
Set FontSize 15
Set Width 900
Set Height 480
Set Shell bash
Set FontFamily "JetBrains Mono"
Set Padding 16

# Setup: create a temporary demo repo with unpinned workflow
Hide
Type `export DEMO=$(mktemp -d) && git init -b main $DEMO && mkdir -p $DEMO/.github/workflows`
Enter
Sleep 2s

Type `printf 'name: CI\non: [push, pull_request]\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps:\n      - uses: actions/checkout@v6\n      - uses: actions/setup-node@v6\n      - uses: actions/cache@v5\n' > $DEMO/.github/workflows/ci.yml`
Enter
Sleep 2s

Type "cd $DEMO"
Enter
Sleep 1s
Type "clear"
Enter
Show

# Show the unpinned workflow
Type "cat .github/workflows/ci.yml"
Enter
Sleep 3s

# Run gx tidy
Type "gx tidy"
Enter
Sleep 8s

# Show the pinned result
Type "cat .github/workflows/ci.yml"
Enter
Sleep 8s
