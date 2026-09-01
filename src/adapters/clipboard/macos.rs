//! Safe macOS typed-pasteboard access through the bounded process adapter.

use std::{ffi::OsString, time::Duration};

use serde::{Deserialize, Serialize};

use crate::ports::environment::{ProcessRequest, ProcessRunner};

use super::{TypedClipboard, TypedSnapshot};

const OSASCRIPT: &str = "/usr/bin/osascript";
const PROCESS_TIMEOUT: Duration = Duration::from_secs(2);

const WRITE_SCRIPT: &str = r"
ObjC.import('AppKit');
ObjC.import('Foundation');
var inputData = $.NSFileHandle.fileHandleWithStandardInput.readDataToEndOfFile;
var inputString = $.NSString.alloc.initWithDataEncoding(inputData, $.NSUTF8StringEncoding);
if (!inputString) throw new Error('clipboard request is not UTF-8');
var request = JSON.parse(ObjC.unwrap(inputString));
var board = $.NSPasteboard.generalPasteboard;
if (!board) throw new Error('general pasteboard is unavailable');
var item = $.NSPasteboardItem.alloc.init;
var typedType = $('dev.proqi.clipboard.annotations-v1');
var textAccepted = item.setStringForType($(request.text), $.NSPasteboardTypeString);
var typedAccepted = item.setStringForType($(request.typed), typedType);
board.clearContents;
var written = board.writeObjects($.NSArray.arrayWithObject(item));
var items = board.pasteboardItems;
var current = items && items.count > 0 ? items.objectAtIndex(0) : null;
var actualText = current ? current.stringForType($.NSPasteboardTypeString) : null;
var actualTyped = current ? current.stringForType(typedType) : null;
var accepted = Boolean(textAccepted && typedAccepted && written && actualText && actualTyped)
    && ObjC.unwrap(actualText) === request.text
    && ObjC.unwrap(actualTyped) === request.typed;
JSON.stringify({ generation: Number(board.changeCount), accepted: accepted });
";

const READ_SCRIPT: &str = r"
ObjC.import('AppKit');
var board = $.NSPasteboard.generalPasteboard;
if (!board) throw new Error('general pasteboard is unavailable');
var items = board.pasteboardItems;
var current = items && items.count > 0 ? items.objectAtIndex(0) : null;
var typedType = $('dev.proqi.clipboard.annotations-v1');
var plain = current ? current.stringForType($.NSPasteboardTypeString) : null;
var typed = current ? current.stringForType(typedType) : null;
JSON.stringify({
    generation: Number(board.changeCount),
    typed: plain && typed ? ObjC.unwrap(typed) : null
});
";

pub(super) struct MacTypedClipboard {
    runner: Box<dyn ProcessRunner + Send>,
}

impl MacTypedClipboard {
    pub(super) fn new(runner: Box<dyn ProcessRunner + Send>) -> Self {
        Self { runner }
    }

    fn run(&mut self, script: &str, stdin: Option<Vec<u8>>) -> Result<Vec<u8>, String> {
        let output = self
            .runner
            .run(ProcessRequest {
                program: OsString::from(OSASCRIPT),
                args: vec![
                    OsString::from("-l"),
                    OsString::from("JavaScript"),
                    OsString::from("-e"),
                    OsString::from(script),
                ],
                stdin,
                timeout: PROCESS_TIMEOUT,
            })
            .map_err(|error| error.to_string())?;
        if output.exit_code != Some(0) {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("macOS pasteboard bridge failed: {}", stderr.trim()));
        }
        Ok(output.stdout)
    }
}

impl TypedClipboard for MacTypedClipboard {
    fn write(&mut self, text: &str, typed: &str) -> Result<u64, String> {
        let request =
            serde_json::to_vec(&WriteRequest { text, typed }).map_err(|error| error.to_string())?;
        let output = self.run(WRITE_SCRIPT, Some(request))?;
        let reply: WriteReply =
            serde_json::from_slice(&output).map_err(|error| error.to_string())?;
        if !reply.accepted {
            return Err("macOS pasteboard did not retain both representations".to_owned());
        }
        Ok(reply.generation)
    }

    fn read(&mut self) -> Result<TypedSnapshot, String> {
        let output = self.run(READ_SCRIPT, None)?;
        let reply: ReadReply =
            serde_json::from_slice(&output).map_err(|error| error.to_string())?;
        Ok(TypedSnapshot {
            generation: reply.generation,
            payload: reply.typed,
        })
    }
}

#[derive(Serialize)]
struct WriteRequest<'a> {
    text: &'a str,
    typed: &'a str,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteReply {
    generation: u64,
    accepted: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadReply {
    generation: u64,
    typed: Option<String>,
}
