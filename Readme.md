# A simplified version of git

> Note: This crate only works on Unix-like systems.

# usage
```bash
Usage: gitlet <COMMAND>

Commands:
  init          init gitlet repository
  cat-file      Provide content of repository objects
  hash-object   Compute objects ID and optionally creates a blob from a file
  log           Display history of a given commit
  ls-tree       List the contents of a tree objects
  checkout      Checkout a commit inside of a directory. todo this just clones file by tree into the directory, does not update HEAD
  show-ref      List all refs in a local repository
  tag           tag
  ls-files      List all the stage files
  check-ignore  Check path(s) against ignore rules
  status        Show the working tree status
  rm            Remove files from the working tree and the index
  add           Add files contents to the index
  commit        Record changes to the repository
  help          Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```
