# Avencrew

A shared workspace for people and AI workers across sales, product management, and engineering.

## Product direction

Avencrew will bring team context, work, and deliverables into one workspace. AI workers will support email, reports, spreadsheets, coding, scheduled work, and long-running tasks.

Sales, product management, and engineering are in scope from the first release. They share projects, tasks, artifacts, workers, and capabilities. Work can begin with any teammate and span several functions; department labels do not partition the product or determine capability access.

Planned capabilities include:

- An owned agent runtime and harness, with model APIs behind it.
- Recoverable task execution, schedules, progress tracking, and human review.
- Plugins, connected tools, and skill creation and import.
- Worker building and shared artifacts across sales, product, and engineering.

## Current status

This repository is a fresh start. The detailed architecture, language choices, and technical specifications are still being evaluated. There is no application or development setup here yet.

## Team-Workspace archive

The previous Team-Workspace project will be copied into a separate folder in this repository as a learning archive. Its code and documents record earlier decisions, research, and lessons; they are not the implementation foundation for Avencrew.

When copying the old project, exclude its nested `.git` directory, credentials, environment files, virtual environments, dependencies, and generated outputs. Keep useful source and planning documents for reference.

New Avencrew plans should live in `docs/`, separate from the old project archive.
