# Summary of a codebase
A tool to dump the whole contents of a codebase to a file.
Uses a declarative configuration TOML file 
for specifying which files/folders to use or not use.

* Generate a template with `summary init summary.toml`,
* Run with `summary run --source summary.toml -o summary.md`

## sample configuration
```toml
# Project name, printed at the top of the output file.
name = "my-project"

# The root directory of the project. All relative paths below are resolved
# from here. Usually the directory that contains this config file.
root = "."

# Individual files to always include, relative to `root`.
files = [
    "README.md",
    "Cargo.toml",
]

# Directories to include recursively, relative to `root`.
# Every file inside (at any depth) is added to the search space.
directories = [
    "src",
]

# Glob patterns, relative to `root`.
# Useful for targeting specific file types across many directories.
globs = [
    "docs/**/*.md",
    "scripts/**/*.sh",
]

# Extension allowlist. Only files whose extension matches an entry here
# will be written to the output. The leading dot is optional.
# Leave the list empty ([]) to allow every extension.
extensions = [
    "rs",
    "toml",
    "md",
    "sh",
    "py",
    "ts",
    "js",
]

# Patterns to always exclude, even if matched by files/directories/globs above.
exclude = [
    "target/**",
    "**/*.lock",
    "**/*.snap",
]
```