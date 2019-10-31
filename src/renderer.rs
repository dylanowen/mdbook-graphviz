use mdbook::errors::ErrorKind;
use mdbook::errors::Result;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

pub trait GraphvizRenderer {
    fn render_graphviz(&self, code: &String, output_path: &PathBuf) -> Result<()>;
}

pub struct CommandLineGraphviz;

impl GraphvizRenderer for CommandLineGraphviz {
    fn render_graphviz(&self, code: &String, output_path: &PathBuf) -> Result<()> {
        let output_path_str = output_path.to_str().ok_or_else(|| {
            ErrorKind::Io(io::Error::new(
                io::ErrorKind::NotFound,
                "Couldn't build output path",
            ))
        })?;

        let mut child = Command::new("dot")
            .args(&["-Tsvg", "-o", output_path_str])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(code.as_bytes())?;
        }

        if child.wait()?.success() {
            Ok(())
        } else {
            Err(ErrorKind::Io(io::Error::new(
                io::ErrorKind::InvalidData,
                "Error response from Graphviz",
            ))
            .into())
        }
    }
}
