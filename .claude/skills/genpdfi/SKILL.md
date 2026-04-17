```markdown
# genpdfi Development Patterns

> Auto-generated skill from repository analysis

## Overview
This skill teaches you the development conventions and workflows used in the `genpdfi` Rust codebase. You'll learn about file naming, import/export styles, commit patterns, and how to write and run tests in this repository. This guide is ideal for contributors looking to maintain consistency and efficiency when working on `genpdfi`.

## Coding Conventions

### File Naming
- **Style:** camelCase
- **Example:**  
  - `pdfGenerator.rs`
  - `textParser.rs`

### Import Style
- **Style:** Relative imports are used to reference modules within the project.
- **Example:**
  ```rust
  mod utils;
  use crate::pdfGenerator::PdfGenerator;
  ```

### Export Style
- **Style:** Named exports are used for exposing functions, structs, or modules.
- **Example:**
  ```rust
  pub struct PdfGenerator { ... }
  pub fn generate_pdf(...) { ... }
  ```

### Commit Patterns
- **Type:** Freeform (no strict prefix or format)
- **Average Length:** 43 characters
- **Example:**
  ```
  Improve PDF rendering for complex tables
  ```

## Workflows

### Adding a New Feature
**Trigger:** When you need to implement a new feature in the codebase  
**Command:** `/add-feature`

1. Create a new file using camelCase naming (e.g., `newFeature.rs`).
2. Implement the feature using relative imports for dependencies.
3. Export new structs/functions using named exports.
4. Write corresponding tests in a file matching `*.test.*`.
5. Commit your changes with a clear, concise message.

### Fixing a Bug
**Trigger:** When you identify and need to fix a bug  
**Command:** `/fix-bug`

1. Locate the relevant module using camelCase file names.
2. Apply the fix, maintaining relative import style.
3. Update or add tests in the corresponding `*.test.*` file.
4. Commit with a descriptive message of the fix.

### Writing and Running Tests
**Trigger:** When developing new features or fixing bugs  
**Command:** `/run-tests`

1. Write tests in files matching the pattern `*.test.*`.
2. Use Rust's standard testing framework (e.g., `#[test]`), as the specific framework is unknown.
3. Run tests using Cargo:
   ```sh
   cargo test
   ```
4. Ensure all tests pass before merging changes.

## Testing Patterns

- **Test File Pattern:** Files are named with the pattern `*.test.*` (e.g., `pdfGenerator.test.rs`).
- **Framework:** Not explicitly specified, but use Rust's built-in test framework.
- **Example:**
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn test_pdf_generation() {
          // Test implementation here
      }
  }
  ```

## Commands
| Command      | Purpose                                      |
|--------------|----------------------------------------------|
| /add-feature | Start the workflow for adding a new feature  |
| /fix-bug     | Start the workflow for fixing a bug          |
| /run-tests   | Run the test suite using Cargo               |
```