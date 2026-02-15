# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Progest is an asset management tool for creators, focusing on unified naming conventions across projects. It provides three interfaces:

- **CLI** - Command-line interface
- **GUI** - Desktop application using Avalonia UI
- **TUI** - Terminal User Interface

## Technology Stack

- **Language**: C# (.NET 10.0)
- **UI Framework**: Avalonia UI (cross-platform desktop)
- **MVVM Library**: CommunityToolkit.Mvvm
- **Database**: SQLite
- **Theme**: ShadUI (planned)

## Architecture

The solution is organized into five projects:

### Progest.Core
Core business logic and domain models. This library contains the fundamental functionality that is shared across all interfaces.

### Progest.Db
Database layer using SQLite with a sidecar-based metadata system (Unity-like asset management). Features include:
- Tag support (add, remove, edit, search, filter)
- Directory-level metadata
- File dependency tracking
- Hybrid approach: sidecar files + SQLite for fast indexing and search

### Progest.Gui
Desktop application using Avalonia UI with MVVM pattern:
- **Views/** - XAML UI files (.axaml)
- **ViewModels/** - ViewModels using CommunityToolkit.Mvvm
- **Models/** - UI-specific models

### Progest.Cli
Command-line interface for terminal-based operation.

### Progest.Tui
Terminal User Interface for rich terminal interaction.

## Key Features

1. **Unified Naming Conventions**: Apply consistent naming across projects and directories
   - Basic conventions: snake_case, camelCase, PascalCase, kebab-case, Title Case
   - Prefix/suffix support (dates, version numbers, etc.)
   - Sequential numbering: 0001_, 0002_, etc.

2. **Template System**: Export directory structures and configurations as templates for reuse or team sharing

3. **Asset Management**: Unity-like sidecar metadata system with:
   - Tag support
   - Directory metadata
   - File dependency tracking with warnings on deletion

4. **AI Features**: Automatic naming convention generation and file organization based on existing patterns

## Development Commands

### Building
```bash
# Build entire solution
dotnet build src/Progest.slnx

# Build specific project
dotnet build src/Progest.Core/Progest.Core.csproj
dotnet build src/Progest.Gui/Progest.Gui.csproj
```

### Running
```bash
# Run GUI application
dotnet run --project src/Progest.Gui/Progest.Gui.csproj

# Run CLI
dotnet run --project src/Progest.Cli/Progest.Cli.csproj

# Run TUI
dotnet run --project src/Progest.Tui/Progest.Tui.csproj
```

### Testing
```bash
# Run all tests (when test projects are added)
dotnet test

# Run specific test project
dotnet test src/Progest.Tests/Progest.Tests.csproj
```

## Development Guidelines

### Communication and Clarity
- **Always ask the user when uncertain**: If any requirement, behavior, or technical decision is unclear, ask the user before proceeding. Never leave ambiguities unresolved.
- **Confirm assumptions**: When implementing features, verify understanding of requirements with the user before writing code.

### Commit Strategy
- **Granular commits**: Commit changes frequently, organized by feature or functionality. Small, focused commits are preferred over large, monolithic changes.
- **Descriptive commits**: Write commit messages that clearly describe what was changed and why, following the repository's commit message style.

### Testing Approach
- **Test-Driven Development (TDD)**: Follow TDD when possible—write tests before implementation. This is especially important for Core and Db projects.
- **GUI Testing**: TDD may be challenging for GUI components due to Avalonia UI testing complexity. Focus TDD efforts on Core, Db, Cli, and Tui projects where unit and integration tests are more straightforward.
- **Test structure**: Organize tests by feature or functionality, mirroring the granular commit approach.

### Documentation
- **Read docs/ before implementation**: Always read files in the `docs/` directory before starting implementation work. This directory contains project-specific documentation, requirements, and design decisions that may not be reflected in the code yet.

### Code Quality
- **Follow existing patterns**: Maintain consistency with the codebase's established patterns and conventions.
- **MVVM adherence**: When working on the GUI, respect the MVVM separation—keep business logic in ViewModels and UI logic in Views.

## Architecture Notes

- **MVVM Pattern**: The GUI follows the Model-View-ViewModel pattern with CommunityToolkit.Mvvm
- **Cross-Platform**: Avalonia UI ensures the GUI works on Windows, macOS, and Linux
- **Solution Format**: Uses .slnx (newer XML-based solution format)
- **Implicit Usings**: Enabled across all projects (reduces need for using statements)
- **Nullable Reference Types**: Enabled for better null safety
