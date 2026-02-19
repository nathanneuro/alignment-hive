use std::path::{Path, PathBuf};

use serde_json::json;

use crate::jupyter::messages::ExecutionOutput;

/// Manages a notebook file (.ipynb) for a single kernel.
pub struct Notebook {
    path: PathBuf,
    cells: Vec<serde_json::Value>,
    execution_count: u32,
}

impl Notebook {
    /// Create a new notebook for a kernel. If `name` is provided, it's used in the filename;
    /// otherwise falls back to a short prefix of the kernel ID.
    /// `notebook_dir` is the directory to store notebooks in (absolute path).
    pub fn new(notebook_dir: &Path, kernel_id: &str, name: Option<&str>) -> anyhow::Result<Self> {
        let dir = notebook_dir;
        std::fs::create_dir_all(dir)?;

        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let label = match name {
            Some(n) => sanitize_filename(n),
            None => kernel_id[..8.min(kernel_id.len())].to_string(),
        };
        let path = dir.join(format!("{timestamp}_{label}.ipynb"));

        let notebook = Self {
            path,
            cells: Vec::new(),
            execution_count: 0,
        };
        notebook.save()?;

        tracing::info!(path = %notebook.path.display(), "Created notebook");
        Ok(notebook)
    }

    /// Get the path to the notebook file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append a code cell with its output (used when execution completes synchronously).
    pub fn append_cell(&mut self, code: &str, output: &ExecutionOutput) -> anyhow::Result<()> {
        self.execution_count += 1;

        let outputs = build_outputs(output, self.execution_count);

        let cell = json!({
            "cell_type": "code",
            "execution_count": self.execution_count,
            "metadata": {},
            "source": split_source(code),
            "outputs": outputs
        });

        self.cells.push(cell);
        self.save()
    }

    /// Create a cell placeholder with empty output. Returns the cell number (1-indexed).
    /// Used for streaming: the cell is created up front, then updated as output arrives.
    pub fn append_cell_placeholder(&mut self, code: &str) -> anyhow::Result<u32> {
        self.execution_count += 1;

        let cell = json!({
            "cell_type": "code",
            "execution_count": self.execution_count,
            "metadata": {},
            "source": split_source(code),
            "outputs": []
        });

        self.cells.push(cell);
        self.save()?;
        Ok(self.execution_count)
    }

    /// Update the output of an existing cell (identified by `cell_number`, 1-indexed).
    /// Called as streaming output arrives and on completion.
    pub fn update_cell_output(
        &mut self,
        cell_number: u32,
        output: &ExecutionOutput,
    ) -> anyhow::Result<()> {
        let index = (cell_number - 1) as usize;
        if let Some(cell) = self.cells.get_mut(index) {
            let outputs = build_outputs(output, cell_number);
            cell["outputs"] = json!(outputs);
            self.save()?;
        }
        Ok(())
    }

    fn save(&self) -> anyhow::Result<()> {
        let notebook = json!({
            "nbformat": 4,
            "nbformat_minor": 5,
            "metadata": {
                "kernelspec": {
                    "display_name": "Python 3",
                    "language": "python",
                    "name": "python3"
                },
                "language_info": {
                    "name": "python",
                    "version": "3.10"
                }
            },
            "cells": self.cells
        });

        let json = serde_json::to_string_pretty(&notebook)?;
        std::fs::write(&self.path, json)?;
        Ok(())
    }
}

/// Split source code into lines for nbformat (each line ends with \n except the last).
fn split_source(code: &str) -> Vec<String> {
    let lines: Vec<&str> = code.split('\n').collect();
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if i < lines.len() - 1 {
                format!("{line}\n")
            } else {
                (*line).to_string()
            }
        })
        .collect()
}

/// Sanitize a user-provided name for use in a filename.
/// Replaces non-alphanumeric characters (except hyphens and underscores) with underscores,
/// and truncates to a reasonable length.
fn sanitize_filename(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('_');
    trimmed[..trimmed.len().min(64)].to_string()
}

/// Build notebook output cells from execution output.
fn build_outputs(output: &ExecutionOutput, execution_count: u32) -> Vec<serde_json::Value> {
    let mut outputs = Vec::new();

    if !output.stdout.is_empty() {
        outputs.push(json!({
            "output_type": "stream",
            "name": "stdout",
            "text": split_source(&output.stdout)
        }));
    }

    if !output.stderr.is_empty() {
        outputs.push(json!({
            "output_type": "stream",
            "name": "stderr",
            "text": split_source(&output.stderr)
        }));
    }

    if let Some(ref result) = output.result {
        outputs.push(json!({
            "output_type": "execute_result",
            "execution_count": execution_count,
            "data": {
                "text/plain": split_source(result)
            },
            "metadata": {}
        }));
    }

    if let Some(ref err) = output.error {
        outputs.push(json!({
            "output_type": "error",
            "ename": err.ename,
            "evalue": err.evalue,
            "traceback": err.traceback
        }));
    }

    outputs
}
