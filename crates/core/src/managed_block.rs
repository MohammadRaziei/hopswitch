//! Safe, reversible editing of text files we do not own, such as `/etc/resolv.conf`.
//!
//! Hopswitch only ever writes **one clearly marked block**. Everything outside it is
//! preserved byte-for-byte, nothing is deleted, and removing the block restores the
//! original file exactly. If the markers look damaged (unterminated, stray, duplicated)
//! the functions refuse to proceed rather than guess.
//!
//! The block is inserted at the **top** of the file so that, for files like
//! `resolv.conf` where order means priority, our entries win while the user's own
//! entries stay below them.
//!
//! These functions work on strings only; reading and writing the file is the
//! responsibility of the platform adapter.

use thiserror::Error;

const BEGIN_PREFIX: &str = "# >>> hopswitch begin";
const END_PREFIX: &str = "# <<< hopswitch end";

/// First line of the block Hopswitch writes.
pub const BEGIN_MARKER: &str =
    "# >>> hopswitch begin (managed by Hopswitch; this whole block is safe to remove) >>>";
/// Last line of the block Hopswitch writes.
pub const END_MARKER: &str = "# <<< hopswitch end <<<";

/// Why a managed-block operation was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BlockError {
    /// A begin marker has no matching end marker.
    #[error("found a Hopswitch begin marker without an end marker; refusing to modify the file")]
    UnterminatedBlock,
    /// An end marker appears without a begin marker.
    #[error("found a Hopswitch end marker without a begin marker; refusing to modify the file")]
    StrayEndMarker,
    /// More than one block exists.
    #[error("found more than one Hopswitch block; refusing to modify the file")]
    DuplicateBlock,
    /// A line cannot be stored safely inside a block.
    #[error("line cannot be written inside a managed block: {0:?}")]
    InvalidLine(String),
}

/// Inserts the managed block at the top of `original`, or replaces the existing one in place.
///
/// An empty `lines` slice is equivalent to [`remove_block`].
pub fn apply_block<S: AsRef<str>>(original: &str, lines: &[S]) -> Result<String, BlockError> {
    for line in lines {
        validate_line(line.as_ref())?;
    }
    let parts = split_lines(original);
    let found = find_block(&parts)?;

    if lines.is_empty() {
        return Ok(strip(&parts, found));
    }

    let block = render(lines);
    if let Some((begin, end)) = found {
        return Ok(format!(
            "{}{block}{}",
            parts[..begin].concat(),
            parts[end + 1..].concat()
        ));
    }
    Ok(format!("{block}{original}"))
}

/// Removes the managed block, leaving everything else untouched.
///
/// Returns the input unchanged when there is no block.
pub fn remove_block(original: &str) -> Result<String, BlockError> {
    let parts = split_lines(original);
    let found = find_block(&parts)?;
    Ok(strip(&parts, found))
}

/// Whether a well-formed managed block is present.
pub fn contains_block(text: &str) -> Result<bool, BlockError> {
    Ok(find_block(&split_lines(text))?.is_some())
}

fn split_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

fn is_begin(line: &str) -> bool {
    line.trim().starts_with(BEGIN_PREFIX)
}

fn is_end(line: &str) -> bool {
    line.trim().starts_with(END_PREFIX)
}

fn validate_line(line: &str) -> Result<(), BlockError> {
    if line.contains(['\n', '\r']) || is_begin(line) || is_end(line) {
        return Err(BlockError::InvalidLine(line.to_owned()));
    }
    Ok(())
}

/// Locates the block as inclusive `(begin, end)` line indices.
fn find_block(lines: &[&str]) -> Result<Option<(usize, usize)>, BlockError> {
    let mut open: Option<usize> = None;
    let mut found: Option<(usize, usize)> = None;

    for (index, line) in lines.iter().enumerate() {
        if is_begin(line) {
            if open.is_some() || found.is_some() {
                return Err(BlockError::DuplicateBlock);
            }
            open = Some(index);
        } else if is_end(line) {
            match open.take() {
                Some(begin) if found.is_none() => found = Some((begin, index)),
                _ => return Err(BlockError::StrayEndMarker),
            }
        }
    }

    if open.is_some() {
        return Err(BlockError::UnterminatedBlock);
    }
    Ok(found)
}

fn strip(parts: &[&str], found: Option<(usize, usize)>) -> String {
    match found {
        Some((begin, end)) => format!("{}{}", parts[..begin].concat(), parts[end + 1..].concat()),
        None => parts.concat(),
    }
}

fn render<S: AsRef<str>>(lines: &[S]) -> String {
    let mut block = format!("{BEGIN_MARKER}\n");
    for line in lines {
        block.push_str(line.as_ref());
        block.push('\n');
    }
    block.push_str(END_MARKER);
    block.push('\n');
    block
}
