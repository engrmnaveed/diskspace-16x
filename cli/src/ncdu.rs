//! Scans saved in the ncdu JSON export format (<https://dev.yorhel.nl/ncdu/jsonfmt>),
//! which ncdu and gdu read and write too. Space 16X adds `space16x_*` keys, which other
//! tools ignore, to keep entry states, issues and names that aren't UTF-8.

use std::borrow::Cow;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::scanner::{
    Entry, EntryKind, EntryState, Identity, Issue, IssueKind, Report, TreeBuilder,
};

/// Writes `report` as an ncdu JSON export, one entry per line, passing it to `output`
/// in chunks.
pub fn write(
    output: &mut dyn FnMut(&[u8]) -> io::Result<()>,
    report: &Report,
    excludes: &[String],
) -> io::Result<()> {
    let out = &mut Vec::with_capacity(CHUNK + 4096);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_secs());
    out.extend_from_slice(b"[1,2,{\"progname\":\"space16x\",\"progver\":\"");
    out.extend_from_slice(env!("CARGO_PKG_VERSION").as_bytes());
    out.extend_from_slice(b"\",\"timestamp\":");
    number(out, timestamp);
    out.extend_from_slice(b",\"space16x_excludedCount\":");
    number(out, report.excluded_count);
    out.extend_from_slice(b",\"space16x_excludes\":[");
    for (index, pattern) in excludes.iter().enumerate() {
        if index > 0 {
            out.push(b',');
        }
        string(out, pattern);
    }
    out.extend_from_slice(b"],\"space16x_issues\":[");
    for (index, issue) in report.issues.iter().enumerate() {
        out.extend_from_slice(if index > 0 { b",\n{" } else { b"\n{" });
        name(
            out,
            b"path",
            b"pathBytes",
            issue.path.as_os_str().as_bytes(),
        );
        out.extend_from_slice(b",\"message\":");
        string(out, &issue.message);
        let kind: &[u8] = match issue.kind {
            IssueKind::PermissionDenied => b"permissionDenied",
            IssueKind::NotPermitted => b"notPermitted",
            IssueKind::Error => b"error",
        };
        out.extend_from_slice(b",\"kind\":\"");
        out.extend_from_slice(kind);
        out.extend_from_slice(b"\"}");
    }
    out.extend_from_slice(b"]},\n[");
    let root = report.root();
    info(out, root, report.tree.root_path().as_os_str().as_bytes());
    // Children iterators of the open folders, so deep trees need no recursion.
    let mut open = vec![root.children()];
    while let Some(children) = open.last_mut() {
        if out.len() >= CHUNK {
            output(out)?;
            out.clear();
        }
        let Some(child) = children.next() else {
            open.pop();
            out.push(b']');
            continue;
        };
        if child.kind() == EntryKind::Directory {
            out.extend_from_slice(b",\n[");
            open.push(child.children());
        } else {
            out.extend_from_slice(b",\n");
        }
        info(out, child, child.name().as_bytes());
    }
    out.extend_from_slice(b"]\n");
    output(out)
}

const CHUNK: usize = 64 * 1024;

fn info(out: &mut Vec<u8>, entry: Entry<'_>, name_bytes: &[u8]) {
    out.push(b'{');
    name(out, b"name", b"space16x_nameBytes", name_bytes);
    if entry.kind() == EntryKind::File {
        out.extend_from_slice(b",\"asize\":");
        number(out, entry.apparent_bytes());
        out.extend_from_slice(b",\"dsize\":");
        number(out, entry.allocated_bytes());
    }
    let (read_error, state): (bool, &[u8]) = match entry.own_state() {
        EntryState::Local => (false, b""),
        EntryState::CloudOnly => (false, b"cloudOnly"),
        EntryState::HardLinkAlias => (false, b"hardLinkAlias"),
        EntryState::Protected => (true, b"protected"),
        EntryState::Incomplete | EntryState::Pending => (true, b""),
    };
    if read_error {
        out.extend_from_slice(b",\"read_error\":true");
    }
    if !state.is_empty() {
        out.extend_from_slice(b",\"space16x_state\":\"");
        out.extend_from_slice(state);
        out.push(b'"');
    }
    out.push(b'}');
}

/// Writes `"key":"name"`, adding the original bytes in hexadecimal under `bytes_key`
/// when they aren't UTF-8.
fn name(out: &mut Vec<u8>, key: &[u8], bytes_key: &[u8], bytes: &[u8]) {
    key_start(out, key);
    let text = String::from_utf8_lossy(bytes);
    string(out, &text);
    if let Cow::Owned(_) = text {
        out.push(b',');
        key_start(out, bytes_key);
        out.push(b'"');
        for &byte in bytes {
            hex(out, byte);
        }
        out.push(b'"');
    }
}

fn key_start(out: &mut Vec<u8>, key: &[u8]) {
    out.push(b'"');
    out.extend_from_slice(key);
    out.extend_from_slice(b"\":");
}

fn number(out: &mut Vec<u8>, mut value: u64) {
    let mut digits = [0; 20];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    out.extend_from_slice(&digits[start..]);
}

fn hex(out: &mut Vec<u8>, byte: u8) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    out.push(DIGITS[usize::from(byte >> 4)]);
    out.push(DIGITS[usize::from(byte & 15)]);
}

fn string(out: &mut Vec<u8>, text: &str) {
    out.push(b'"');
    for &byte in text.as_bytes() {
        match byte {
            b'"' | b'\\' => out.extend_from_slice(&[b'\\', byte]),
            0..0x20 => {
                out.extend_from_slice(b"\\u00");
                hex(out, byte);
            }
            _ => out.push(byte),
        }
    }
    out.push(b'"');
}

/// A scan read from an export, with the exclude patterns it was made with.
pub struct Imported {
    pub report: Report,
    pub excludes: Vec<String>,
}

/// Reads an ncdu JSON export from ncdu, gdu or Space 16X.
///
/// Folder totals count their contents, as a Space 16X scan does, so a folder's own size
/// in the file isn't added. Excluded, special and other-filesystem entries are left out,
/// and a file with the same device and inode as an earlier one is a hard-link alias.
pub fn read(input: &mut dyn FnMut(&mut [u8]) -> io::Result<usize>) -> io::Result<Imported> {
    let parser = &mut Parser {
        input,
        buffer: vec![0; 64 * 1024].into_boxed_slice(),
        start: 0,
        end: 0,
        line: 1,
        text: Vec::new(),
    };
    parser.skip_space()?;
    match parser.peek()? {
        Some(b'[') => {}
        Some(0xbf) => {
            return Err(invalid(
                "This is an ncdu binary export. Export JSON from ncdu with -o instead of -O.",
            ));
        }
        _ => return Err(invalid("This isn't an ncdu JSON export.")),
    }
    parser.next()?;
    let major = parser.number()?;
    if major != 1 {
        return Err(invalid(format!(
            "This ncdu export uses format version {major}, which Space 16X can't read."
        )));
    }
    parser.expect(b',')?;
    parser.number()?;
    parser.expect(b',')?;
    let mut excludes = Vec::new();
    let mut excluded_count = None;
    let mut issues = None;
    let mut program = Vec::new();
    parser.object(&mut |parser, key| {
        match key {
            b"progname" => program = parser.string()?,
            b"space16x_excludedCount" => excluded_count = Some(parser.number()?),
            b"space16x_excludes" => parser.array(&mut |parser| {
                let pattern = parser.string()?;
                excludes.push(String::from_utf8_lossy(&pattern).into_owned());
                Ok(())
            })?,
            b"space16x_issues" => {
                let list = issues.insert(Vec::new());
                parser.array(&mut |parser| {
                    list.push(parser.issue()?);
                    Ok(())
                })?;
            }
            _ => parser.skip_value()?,
        }
        Ok(())
    })?;
    parser.expect(b',')?;
    parser.expect(b'[')?;
    let root = parser.info()?;
    let root_path = PathBuf::from(OsStr::from_bytes(&root.name));
    if root_path.as_os_str().is_empty() {
        return Err(parser.error("the scan root has no name"));
    }
    // Space 16X exports list every issue, so only other tools' read errors become issues.
    let issues_from_entries = issues.is_none();
    let mut issues = issues.unwrap_or_default();
    if issues_from_entries && root.read_error {
        issues.push(read_error(root_path.clone()));
    }
    let mut builder = TreeBuilder::new(root_path.clone(), root.own_state());
    let mut path = root_path;
    let mut devices = vec![root.device.unwrap_or(0)];
    let mut links = HashSet::new();
    // gdu marks only the second and later links to a file, so each one is an alias.
    // ncdu marks every link, so only a repeated device and inode is.
    let gdu = program == b"gdu";
    let mut pattern_matches = 0;
    loop {
        match parser.token()? {
            b',' => {}
            b']' if devices.len() == 1 => break,
            b']' => {
                devices.pop();
                path.pop();
                builder.close_folder();
                continue;
            }
            _ => return Err(parser.error("expected ',' or ']'")),
        }
        parser.skip_space()?;
        let folder = parser.peek()? == Some(b'[');
        if folder {
            parser.next()?;
        }
        let entry = parser.info()?;
        let name = entry.name.as_slice();
        if name.is_empty() || name == b"." || name == b".." || name.contains(&b'/') {
            return Err(parser.error("an entry has an invalid name"));
        }
        if entry.excluded.is_some() || entry.special {
            // Patterns count as exclusions. Other filesystems and special files are
            // skipped quietly, as a scan skips them.
            if entry.excluded.as_deref().is_some_and(|kind| {
                !matches!(
                    kind,
                    b"otherfs" | b"othfs" | b"kernfs" | b"frmlnk" | b"frmlink"
                )
            }) {
                pattern_matches += 1;
            }
            if folder {
                parser.skip_folder()?;
            }
            continue;
        }
        let state = entry.own_state();
        let device = entry.device.unwrap_or(devices[devices.len() - 1]);
        if issues_from_entries && entry.read_error {
            issues.push(read_error(path.join(OsStr::from_bytes(name))));
        }
        if folder {
            builder.open_folder(name, state);
            path.push(OsStr::from_bytes(name));
            devices.push(device);
        } else if state == EntryState::HardLinkAlias
            || (gdu && entry.hard_link)
            || (!gdu
                && (entry.hard_link || entry.links > 1)
                && !links.insert(Identity::new(device, entry.inode)))
        {
            builder.file(name, 0, 0, EntryState::HardLinkAlias);
        } else {
            builder.file(name, entry.allocated, entry.apparent, state);
        }
    }
    parser.expect(b']')?;
    parser.skip_space()?;
    if parser.peek()?.is_some() {
        return Err(parser.error("unexpected data after the export"));
    }
    let (tree, _) = builder.try_finish()?;
    Ok(Imported {
        report: Report {
            tree: Arc::new(tree),
            issues,
            excluded_count: excluded_count.unwrap_or(pattern_matches),
            is_preview: false,
        },
        excludes,
    })
}

fn read_error(path: PathBuf) -> Issue {
    Issue {
        path,
        message: "It couldn't be read when the scan was made.".into(),
        kind: IssueKind::Error,
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[derive(Default)]
struct Info {
    name: Vec<u8>,
    apparent: u64,
    allocated: u64,
    device: Option<u64>,
    inode: u64,
    links: u64,
    hard_link: bool,
    read_error: bool,
    special: bool,
    excluded: Option<Vec<u8>>,
    state: Option<EntryState>,
}

impl Info {
    fn own_state(&self) -> EntryState {
        match self.state {
            Some(state) => state,
            None if self.read_error => EntryState::Incomplete,
            None => EntryState::Local,
        }
    }
}

/// A streaming JSON reader, so an export never needs to fit in memory at once. Strings
/// may hold bytes that aren't UTF-8, because ncdu writes names as the filesystem has them.
struct Parser<'a> {
    /// Reads more bytes, like `Read::read`.
    input: &'a mut dyn FnMut(&mut [u8]) -> io::Result<usize>,
    buffer: Box<[u8]>,
    start: usize,
    end: usize,
    line: usize,
    /// Reused for keys and other strings.
    text: Vec<u8>,
}

impl Parser<'_> {
    fn peek(&mut self) -> io::Result<Option<u8>> {
        if self.start == self.end {
            self.start = 0;
            self.end = loop {
                match (self.input)(&mut self.buffer) {
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    result => break result?,
                }
            };
        }
        Ok(self.buffer[self.start..self.end].first().copied())
    }

    fn next(&mut self) -> io::Result<u8> {
        let byte = self
            .peek()?
            .ok_or_else(|| self.error("the export ends early"))?;
        self.start += 1;
        if byte == b'\n' {
            self.line += 1;
        }
        Ok(byte)
    }

    fn error(&self, message: &str) -> io::Error {
        invalid(format!("line {}: {message}", self.line))
    }

    fn skip_space(&mut self) -> io::Result<()> {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.peek()? {
            self.next()?;
        }
        Ok(())
    }

    /// The next byte after any whitespace.
    fn token(&mut self) -> io::Result<u8> {
        self.skip_space()?;
        self.next()
    }

    fn expect(&mut self, byte: u8) -> io::Result<()> {
        if self.token()? == byte {
            Ok(())
        } else {
            Err(self.error(&format!("expected '{}'", byte as char)))
        }
    }

    fn number(&mut self) -> io::Result<u64> {
        self.skip_space()?;
        let mut value: u64 = 0;
        let mut digits = 0;
        while let Some(byte @ b'0'..=b'9') = self.peek()? {
            self.next()?;
            digits += 1;
            value = value
                .checked_mul(10)
                .and_then(|value| value.checked_add(u64::from(byte - b'0')))
                .ok_or_else(|| self.error("a number is too large"))?;
        }
        if digits == 0 || matches!(self.peek()?, Some(b'.' | b'e' | b'E')) {
            return Err(self.error("expected a whole number"));
        }
        Ok(value)
    }

    fn boolean(&mut self) -> io::Result<bool> {
        let first = self.token()?;
        while let Some(b'a'..=b'z') = self.peek()? {
            self.next()?;
        }
        match first {
            b't' => Ok(true),
            b'f' | b'n' => Ok(false),
            _ => Err(self.error("expected true or false")),
        }
    }

    /// Reads a string into `self.text` and returns a copy.
    fn string(&mut self) -> io::Result<Vec<u8>> {
        self.expect(b'"')?;
        self.read_string()?;
        Ok(self.text.clone())
    }

    /// Reads the rest of a string, after its opening quote, into `self.text`.
    fn read_string(&mut self) -> io::Result<()> {
        self.text.clear();
        loop {
            let byte = match self.next()? {
                b'"' => return Ok(()),
                b'\\' => match self.next()? {
                    b'b' => 8,
                    b'f' => 12,
                    b'n' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    b'u' => {
                        let mut code = self.hex4()?;
                        if (0xd800..0xdc00).contains(&code) && self.peek()? == Some(b'\\') {
                            self.next()?;
                            if self.next()? != b'u' {
                                return Err(self.error("invalid escape"));
                            }
                            let low = self.hex4()?;
                            code = if (0xdc00..0xe000).contains(&low) {
                                0x10000 + ((code - 0xd800) << 10) + (low - 0xdc00)
                            } else {
                                self.push_char(0xfffd);
                                low
                            };
                        }
                        self.push_char(code);
                        continue;
                    }
                    other @ (b'"' | b'\\' | b'/') => other,
                    _ => return Err(self.error("invalid escape")),
                },
                other => other,
            };
            self.text.push(byte);
        }
    }

    fn push_char(&mut self, code: u32) {
        let character = char::from_u32(code).unwrap_or(char::REPLACEMENT_CHARACTER);
        let mut encoded = [0; 4];
        self.text
            .extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
    }

    fn hex4(&mut self) -> io::Result<u32> {
        let mut code = 0;
        for _ in 0..4 {
            let digit = char::from(self.next()?)
                .to_digit(16)
                .ok_or_else(|| self.error("invalid escape"))?;
            code = code * 16 + digit;
        }
        Ok(code)
    }

    /// Decodes a string of hexadecimal byte pairs.
    fn hex_bytes(&mut self) -> io::Result<Vec<u8>> {
        let text = self.string()?;
        let mut bytes = Vec::with_capacity(text.len() / 2);
        for pair in text.chunks(2) {
            let digit = |byte: u8| char::from(byte).to_digit(16);
            match (digit(pair[0]), pair.get(1).copied().and_then(digit)) {
                (Some(high), Some(low)) => bytes.push((high * 16 + low) as u8),
                _ => return Err(self.error("invalid hexadecimal bytes")),
            }
        }
        Ok(bytes)
    }

    /// Skips any value. Commas and colons between values inside it aren't checked.
    fn skip_value(&mut self) -> io::Result<()> {
        let mut depth = 0_usize;
        loop {
            match self.token()? {
                b'"' => self.read_string()?,
                b'[' | b'{' => depth += 1,
                b']' | b'}' => {
                    depth = depth
                        .checked_sub(1)
                        .ok_or_else(|| self.error("unexpected ']' or '}'"))?;
                }
                _ => {
                    while let Some(byte) = self.peek()?
                        && !b",:[]{}\" \t\r\n".contains(&byte)
                    {
                        self.next()?;
                    }
                }
            }
            if depth == 0 {
                return Ok(());
            }
        }
    }

    /// Skips the rest of a folder's array after its info object.
    fn skip_folder(&mut self) -> io::Result<()> {
        loop {
            match self.token()? {
                b',' => self.skip_value()?,
                b']' => return Ok(()),
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn array(&mut self, item: &mut dyn FnMut(&mut Self) -> io::Result<()>) -> io::Result<()> {
        self.expect(b'[')?;
        self.skip_space()?;
        if self.peek()? == Some(b']') {
            self.next()?;
            return Ok(());
        }
        loop {
            item(self)?;
            match self.token()? {
                b',' => {}
                b']' => return Ok(()),
                _ => return Err(self.error("expected ',' or ']'")),
            }
        }
    }

    fn object(
        &mut self,
        field: &mut dyn FnMut(&mut Self, &[u8]) -> io::Result<()>,
    ) -> io::Result<()> {
        self.expect(b'{')?;
        self.skip_space()?;
        if self.peek()? == Some(b'}') {
            self.next()?;
            return Ok(());
        }
        loop {
            let key = self.string()?;
            self.expect(b':')?;
            field(self, &key)?;
            match self.token()? {
                b',' => {}
                b'}' => return Ok(()),
                _ => return Err(self.error("expected ',' or '}'")),
            }
        }
    }

    fn info(&mut self) -> io::Result<Info> {
        let mut info = Info {
            links: 1,
            ..Info::default()
        };
        let mut original_name = None;
        self.object(&mut |parser, key| {
            match key {
                b"name" => info.name = parser.string()?,
                b"asize" => info.apparent = parser.number()?,
                b"dsize" => info.allocated = parser.number()?,
                b"dev" => info.device = Some(parser.number()?),
                b"ino" => info.inode = parser.number()?,
                b"nlink" => info.links = parser.number()?,
                b"hlnkc" => info.hard_link = parser.boolean()?,
                b"read_error" => info.read_error = parser.boolean()?,
                b"notreg" => info.special = parser.boolean()?,
                b"excluded" => info.excluded = Some(parser.string()?),
                b"space16x_nameBytes" => original_name = Some(parser.hex_bytes()?),
                b"space16x_state" => {
                    info.state = match parser.string()?.as_slice() {
                        b"cloudOnly" => Some(EntryState::CloudOnly),
                        b"hardLinkAlias" => Some(EntryState::HardLinkAlias),
                        b"protected" => Some(EntryState::Protected),
                        b"incomplete" => Some(EntryState::Incomplete),
                        _ => None,
                    }
                }
                _ => parser.skip_value()?,
            }
            Ok(())
        })?;
        if let Some(name) = original_name {
            info.name = name;
        }
        Ok(info)
    }

    fn issue(&mut self) -> io::Result<Issue> {
        let mut path = Vec::new();
        let mut original_path = None;
        let mut message = String::new();
        let mut kind = IssueKind::Error;
        self.object(&mut |parser, key| {
            match key {
                b"path" => path = parser.string()?,
                b"pathBytes" => original_path = Some(parser.hex_bytes()?),
                b"message" => message = String::from_utf8_lossy(&parser.string()?).into_owned(),
                b"kind" => {
                    kind = match parser.string()?.as_slice() {
                        b"permissionDenied" => IssueKind::PermissionDenied,
                        b"notPermitted" => IssueKind::NotPermitted,
                        _ => IssueKind::Error,
                    }
                }
                _ => parser.skip_value()?,
            }
            Ok(())
        })?;
        Ok(Issue {
            path: PathBuf::from(OsStr::from_bytes(&original_path.unwrap_or(path))),
            message,
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::scanner::Tree;

    /// Reads `text` a few bytes at a time, so values span buffer refills.
    fn import(mut text: &[u8]) -> io::Result<Imported> {
        read(&mut |buffer| {
            let length = text.len().min(buffer.len()).min(7);
            buffer[..length].copy_from_slice(&text[..length]);
            text = &text[length..];
            Ok(length)
        })
    }

    fn export(report: &Report, excludes: &[String]) -> Vec<u8> {
        let mut output = Vec::new();
        write(
            &mut |bytes| {
                output.extend_from_slice(bytes);
                Ok(())
            },
            report,
            excludes,
        )
        .unwrap();
        output
    }

    /// Every node as (path, allocated, apparent, items, state, own state).
    fn nodes(tree: &Tree) -> Vec<(PathBuf, u64, u64, u64, EntryState, EntryState)> {
        std::iter::once(tree.root())
            .chain(tree.root().descendants())
            .map(|entry| {
                (
                    entry.path(),
                    entry.allocated_bytes(),
                    entry.apparent_bytes(),
                    entry.item_count(),
                    entry.state(),
                    entry.own_state(),
                )
            })
            .collect()
    }

    fn sample() -> Report {
        let mut builder = TreeBuilder::new("/data/scan \"root\"".into(), EntryState::Local);
        builder.open_folder(b"a", EntryState::Local);
        builder.file(b"x", 4096, 10, EntryState::Local);
        builder.file(b"alias", 0, 0, EntryState::HardLinkAlias);
        builder.open_folder(b"deep\\er", EntryState::Local);
        builder.file(b"tab\there\n", 8192, 5000, EntryState::Local);
        builder.close_folder();
        builder.close_folder();
        builder.file(b"caf\xe9.txt", 100, 100, EntryState::Local);
        builder.folder(b"cloud", EntryState::CloudOnly);
        builder.file(b"placeholder", 0, 0, EntryState::CloudOnly);
        builder.open_folder(b"locked", EntryState::Protected);
        builder.close_folder();
        builder.open_folder(b"broken", EntryState::Incomplete);
        builder.file(b"kept", 1, 1, EntryState::Local);
        builder.close_folder();
        Report {
            tree: Arc::new(builder.finish().0),
            issues: vec![
                Issue {
                    path: "/data/scan \"root\"/locked".into(),
                    message: "List: Permission denied (os error 13)".into(),
                    kind: IssueKind::PermissionDenied,
                },
                Issue {
                    path: PathBuf::from(OsStr::from_bytes(b"/data/scan \"root\"/bad\xff")),
                    message: "Stat: Input/output error".into(),
                    kind: IssueKind::Error,
                },
            ],
            excluded_count: 3,
            is_preview: false,
        }
    }

    #[test]
    fn round_trips_its_own_exports() {
        let report = sample();
        let excludes = vec!["node_modules".to_owned(), "*.log".to_owned()];
        let output = export(&report, &excludes);
        let text = String::from_utf8(output.clone()).expect("exports are UTF-8");
        assert!(
            text.starts_with("[1,2,{\"progname\":\"space16x\""),
            "{text}"
        );
        let lossy = "{\"name\":\"caf\u{fffd}.txt\",\"space16x_nameBytes\":\"636166e92e747874\",\"asize\":100,\"dsize\":100}";
        assert!(text.contains(lossy), "{text}");
        assert!(
            text.contains(r#"{"name":"tab\u0009here\u000a","asize":5000,"dsize":8192}"#),
            "{text}"
        );
        assert!(
            text.contains(r#"{"name":"locked","read_error":true,"space16x_state":"protected"}"#),
            "{text}"
        );
        let imported = import(&output).unwrap();
        assert_eq!(nodes(&imported.report.tree), nodes(&report.tree));
        assert_eq!(imported.excludes, excludes);
        assert_eq!(imported.report.excluded_count, 3);
        let issues: Vec<_> = imported
            .report
            .issues
            .iter()
            .map(|issue| (issue.path.clone(), issue.message.clone(), issue.kind))
            .collect();
        let expected: Vec<_> = report
            .issues
            .iter()
            .map(|issue| (issue.path.clone(), issue.message.clone(), issue.kind))
            .collect();
        assert_eq!(issues, expected);
        assert!(!imported.report.is_preview);
    }

    #[test]
    fn reads_ncdu_and_gdu_exports() {
        let ncdu = b"[1,2,{\"progname\":\"ncdu\",\"progver\":\"1.19\",\"timestamp\":1700000000,\
            \"space16x_excludes\":[\"a\\/b\"]},\n\
            [{\"name\":\"/srv\",\"asize\":4096,\"dsize\":4096,\"dev\":2049,\"ino\":2},\n\
            {\"name\":\"one\",\"asize\":10,\"dsize\":4096,\"ino\":12,\"hlnkc\":true,\"nlink\":2},\n\
            [{\"name\":\"sub\",\"asize\":4096,\"dsize\":4096,\"extra\":{\"a\":[1,{\"b\":\"]\"}]}},\n\
             {\"name\":\"two\",\"asize\":10,\"dsize\":4096,\"ino\":12,\"nlink\":2},\n\
             {\"name\":\"other\",\"asize\":10,\"dsize\":4096,\"dev\":7,\"ino\":12,\"nlink\":2},\n\
             {\"name\":\"link\",\"asize\":7,\"dsize\":0,\"notreg\":true},\n\
             {\"name\":\"caf\xe9\",\"asize\":1,\"dsize\":512,\"mtime\":1700000000.25}],\n\
            [{\"name\":\"skipped\",\"excluded\":\"pattern\"},[{\"name\":\"x\"}],{\"name\":\"y\"}],\n\
            {\"name\":\"proc\",\"excluded\":\"otherfs\"},\n\
            {\"name\":\"node_modules\",\"excluded\":\"pattern\"},\n\
            [{\"name\":\"denied\",\"read_error\":true}],\n\
            {\"name\":\"\\u00e9\\ud83d\\ude00 \\\"q\\\"\",\"asize\":3,\"dsize\":0}]]\n";
        let imported = import(ncdu).unwrap();
        let report = &imported.report;
        let tree = &report.tree;
        let entry =
            |path: &[u8]| tree.entry(tree.find(Path::new(OsStr::from_bytes(path))).unwrap());
        // Folder sizes come from their contents only.
        assert_eq!(report.root().allocated_bytes(), 4096 + 4096 + 512);
        assert_eq!(entry(b"/srv/sub/two").state(), EntryState::HardLinkAlias);
        assert_eq!(entry(b"/srv/sub/two").allocated_bytes(), 0);
        // The same inode on another device is another file.
        assert_eq!(entry(b"/srv/sub/other").allocated_bytes(), 4096);
        assert!(tree.find(Path::new("/srv/sub/link")).is_none());
        assert_eq!(entry(b"/srv/sub/caf\xe9").apparent_bytes(), 1);
        assert!(tree.find(Path::new("/srv/skipped")).is_none());
        assert!(tree.find(Path::new("/srv/proc")).is_none());
        assert_eq!(report.excluded_count, 2);
        assert_eq!(entry(b"/srv/denied").state(), EntryState::Incomplete);
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].path, Path::new("/srv/denied"));
        assert_eq!(entry("/srv/é😀 \"q\"".as_bytes()).apparent_bytes(), 3);
        assert_eq!(report.root().item_count(), 6);
        assert_eq!(imported.excludes, ["a/b"]);

        // gdu marks only later links, without the first one's inode.
        let gdu = br#"[1,2,{"progname":"gdu","progver":"v5.29.0","timestamp":1700000000},
            [{"name":"/home/me","mtime":1700000000},{"name":"a.bin","asize":5,"dsize":4096,"mtime":1700000000},
            {"name":"b.bin","asize":5,"dsize":4096,"ino":77,"hlnkc":true},
            [{"name":"empty","mtime":1700000000}]]]"#;
        let imported = import(gdu).unwrap();
        assert_eq!(imported.report.root().allocated_bytes(), 4096);
        assert_eq!(imported.report.root().item_count(), 2);
        let tree = &imported.report.tree;
        let alias = tree.find(Path::new("/home/me/b.bin")).unwrap();
        assert_eq!(tree.entry(alias).state(), EntryState::HardLinkAlias);
    }

    /// The same table drives `testMatchesTheSharedExportFixtures` in the Swift package.
    #[test]
    fn matches_the_shared_export_fixtures() {
        use serde_json::{Value, json};
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/scan-exports");
        let expected: Value =
            serde_json::from_slice(&std::fs::read(fixtures.join("expected.json")).unwrap())
                .unwrap();
        let summary = |entry: Entry<'_>| {
            let mut value = json!({
                "path": entry.path().to_str().unwrap(),
                "allocatedBytes": entry.allocated_bytes(),
                "apparentBytes": entry.apparent_bytes(),
                "itemCount": entry.item_count(),
            });
            for (state, flag) in [
                (EntryState::HardLinkAlias, "hardLinkAlias"),
                (EntryState::CloudOnly, "cloudOnly"),
            ] {
                if entry.state() == state {
                    value[flag] = Value::Bool(true);
                }
            }
            value
        };
        let cases = expected["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 3);
        for case in cases {
            let file = case["file"].as_str().unwrap();
            let imported = import(&std::fs::read(fixtures.join(file)).unwrap()).unwrap();
            let report = &imported.report;
            assert_eq!(summary(report.root()), case["root"], "{file}");
            let entries: Vec<_> = report.root().descendants().map(summary).collect();
            assert_eq!(Value::from(entries), case["entries"], "{file}");
            assert_eq!(report.excluded_count, case["excludedCount"], "{file}");
            assert_eq!(json!(imported.excludes), case["excludes"], "{file}");
            let issues: Vec<_> = report
                .issues
                .iter()
                .map(|issue| {
                    let kind = match issue.kind {
                        IssueKind::PermissionDenied => "permissionDenied",
                        IssueKind::NotPermitted => "notPermitted",
                        IssueKind::Error => "error",
                    };
                    json!({"path": issue.path.to_str().unwrap(), "kind": kind})
                })
                .collect();
            assert_eq!(Value::from(issues), case["issues"], "{file}");
        }
    }

    #[test]
    fn rejects_files_it_cannot_read() {
        for (text, message) in [
            (&b"\xbfncduEX1"[..], "ncdu binary export"),
            (b"{}", "isn't an ncdu JSON export"),
            (b"[2,0,{},[{\"name\":\"/\"}]]", "format version 2"),
            (
                b"[1,0,{},[{\"name\":\"/\"},{\"name\":\"a\"}",
                "line 1: the export ends early",
            ),
            (b"[1,0,{},[{\"name\":\"/\"}]]x", "unexpected data"),
            (
                b"[1,0,{},[{\"name\":\"/\"},{\"name\":\"a/b\"}]]",
                "invalid name",
            ),
            (
                b"[1,0,{},[{\"name\":\"/\"},\n{\"name\":\"..\"}]]",
                "line 2: an entry has an invalid name",
            ),
            (b"[1,0,{},[{\"name\":\"\"}]]", "no name"),
            (
                b"[1,0,{},[{\"name\":\"/\"},{\"name\":\"a\",\"asize\":-1}]]",
                "whole number",
            ),
            (
                b"[1,0,{},[{\"name\":\"/\"},{\"name\":\"a\",\"dsize\":1.5}]]",
                "whole number",
            ),
            (
                b"[1,0,{},[{\"name\":\"/\"},{\"name\":\"\\x\"}]]",
                "invalid escape",
            ),
        ] {
            let error = import(text)
                .err()
                .unwrap_or_else(|| panic!("{text:?} should fail"));
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert!(error.to_string().contains(message), "{error} for {text:?}");
        }
    }
}
