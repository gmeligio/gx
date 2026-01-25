# Business logic code

Language Rust. Single binary.

# Principles

- Convention over configuration
- Great Developer Experience
- Intuitive
- Follows known Github versions conventions


# Initial Plan

Let's focus on the simplest case first:
1. Only worry about the `gv set` command.
2. Only think about the `action` global section.
3. Assume there would be a single version of each action throught the repository.
4. When running `gv set`, it will update all the .github/workflows with the specified versions.

# Roadmap

## Manifest and Configuration file

- .github/gv.toml
- There would be a main section of action versions as the typical case where someone uses the same versions across all workflows and actions.
- The configuration should follow the known github conventions. Specifically, support separating the versions by workflow and actions.

## Discover existing versions

1. The discover command will parse .github/workflows and .github/actions.yml for versions. Then, it will merge the existing content of .github/gv.toml with the parsed versions.

## Set configuration

1. The set command will read the manifest file and update the .github/workflows and .github/actions.yml files with the new versions.
