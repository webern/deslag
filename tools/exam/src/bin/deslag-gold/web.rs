//! The review in a browser: the terminal review's [`Session`], served as a page on this machine.
//!
//! The page shows one sentence at a time, with menus of tags in plain words for each word. Saving
//! and rejecting go through the session, so the file is written as `review` writes it, and
//! reopening resumes where the owner left off. When every sentence is done the page can run `own`.
//!
//! The server listens on 127.0.0.1 only and answers only a `Host` that names it, which stops a site
//! that resolves its own name to this machine. A change must be sent as JSON, which a page from
//! another site cannot send without a preflight this server never grants.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use deslag_exam::conllu;
use deslag_exam::error::Error;
use deslag_exam::gold::kind_name;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::code::Code;
use crate::data::read_text;
use crate::guide::GUIDE;
use crate::problems::Problems;
use crate::review::{Row, Session, Store, refusal, refuses_path};
use crate::terminal::{FileStore, today};

/// The page, with its styles and script.
const PAGE: &str = include_str!("web.html");

/// The most bytes a request may carry, for its headers and for its body.
const LIMIT: usize = 1 << 20;

/// Serves `file` for review on `port` until the process is stopped. `into` is where `own` moves
/// the finished sentences.
pub fn run(file: &Path, port: u16, into: &Path) -> Result<(), Problems> {
    let shown = file.display().to_string();
    let canonical = std::fs::canonicalize(file).ok();
    if refuses_path(file) || canonical.as_deref().is_some_and(refuses_path) {
        return Err(refusal(&shown).into());
    }
    let source = read_text(file)?;
    let notes = conllu::read(&shown, &source)?
        .iter()
        .map(Note::of)
        .collect();
    let session = Session::open(&shown, source, &today())?;
    let address = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&address).map_err(|source| Error::Io {
        path: address.clone(),
        source,
    })?;
    let review = Arc::new(Mutex::new(Review {
        session,
        store: FileStore {
            path: file.to_path_buf(),
        },
        file: shown,
        queue: file.to_path_buf(),
        into: into.to_path_buf(),
        notes,
        port,
    }));
    println!("open http://{address}/ in Chrome");
    println!("every sentence is saved to the file as you go; Ctrl-C stops the server");
    for stream in listener.incoming().flatten() {
        let review = Arc::clone(&review);
        std::thread::spawn(move || serve(&stream, &review));
    }
    Ok(())
}

/// What the page shows of a sentence that the session does not keep.
struct Note {
    source: Option<String>,
    repo: Option<String>,
    /// Whether each line is followed by a space, so the page can lay the tokens out as text.
    spaces: Vec<bool>,
}

impl Note {
    fn of(block: &conllu::Block) -> Note {
        let comment = |key: &str| block.comment(key).map(|comment| comment.value.clone());
        Note {
            source: comment("source"),
            repo: comment("repo"),
            spaces: block
                .lines
                .iter()
                .map(|line| !conllu::pairs(&line.misc).contains(&("SpaceAfter", "No")))
                .collect(),
        }
    }
}

/// The open file, and everything a request needs.
struct Review<S> {
    session: Session,
    store: S,
    file: String,
    queue: PathBuf,
    into: PathBuf,
    notes: Vec<Note>,
    port: u16,
}

/// A request, as far as the server reads it.
#[derive(Debug, Default)]
struct Request {
    method: String,
    path: String,
    host: Option<String>,
    json: bool,
    body: Vec<u8>,
}

/// A response to send.
#[derive(Debug)]
struct Response {
    status: u16,
    kind: &'static str,
    body: String,
}

impl Response {
    fn json(status: u16, value: &Value) -> Response {
        Response {
            status,
            kind: "application/json",
            body: value.to_string(),
        }
    }

    fn error(status: u16, message: &str) -> Response {
        Response::json(status, &json!({ "error": message }))
    }
}

/// The body of `POST /save`: the sentence, a tag or `null` for each of its lines, and whether each
/// was taken from deslag's guess.
#[derive(Deserialize)]
struct Save {
    sentence: usize,
    tags: Vec<Option<String>>,
    #[serde(default)]
    accepted: Vec<bool>,
}

/// The body of `POST /reject`.
#[derive(Deserialize)]
struct Reject {
    sentence: usize,
}

impl<S: Store> Review<S> {
    fn handle(&mut self, request: &Request) -> Response {
        let local = request.host.as_deref().is_some_and(|host| {
            host == format!("127.0.0.1:{}", self.port) || host == format!("localhost:{}", self.port)
        });
        if !local {
            return Response::error(403, "this server answers 127.0.0.1 only");
        }
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/") => Response {
                status: 200,
                kind: "text/html; charset=utf-8",
                body: PAGE.to_string(),
            },
            ("GET", "/state") => Response::json(200, &self.state()),
            ("POST", _) if !request.json => Response::error(415, "a change is sent as JSON"),
            ("POST", "/save") => self.save(&request.body),
            ("POST", "/reject") => self.reject(&request.body),
            ("POST", "/own") => match crate::own_stage(&self.queue, &self.into) {
                Ok(moved) => Response::json(200, &json!({ "message": moved })),
                Err(problems) => Response::error(409, &problems.to_string()),
            },
            _ => Response::error(404, "no such page"),
        }
    }

    fn save(&mut self, body: &[u8]) -> Response {
        let save: Save = match serde_json::from_slice(body) {
            Ok(save) => save,
            Err(error) => return Response::error(400, &error.to_string()),
        };
        let tags = save
            .tags
            .iter()
            .map(|tag| tag.as_deref().map(Code::parse).transpose())
            .collect::<Result<Vec<_>, _>>();
        let saved = tags.and_then(|tags| {
            self.session
                .save_tags(save.sentence, &tags, &save.accepted, &mut self.store)
        });
        self.changed(saved)
    }

    fn reject(&mut self, body: &[u8]) -> Response {
        match serde_json::from_slice::<Reject>(body) {
            Ok(reject) => {
                let saved = self.session.reject_at(reject.sentence, &mut self.store);
                self.changed(saved)
            }
            Err(error) => Response::error(400, &error.to_string()),
        }
    }

    /// The answer to a change: the state as it now is, or why the file was not saved.
    fn changed(&self, saved: Result<Option<String>, String>) -> Response {
        match saved {
            Ok(warning) => {
                Response::json(200, &json!({ "warning": warning, "state": self.state() }))
            }
            Err(message) => Response::error(409, &format!("not saved: {message}")),
        }
    }

    /// Everything the page draws.
    fn state(&self) -> Value {
        let sentences: Vec<Value> = self
            .session
            .sentences
            .iter()
            .zip(&self.notes)
            .map(|(sentence, note)| {
                let rows: Vec<Value> = sentence
                    .rows
                    .iter()
                    .zip(&note.spaces)
                    .map(|(row, space)| match row {
                        Row::Word(word) => json!({
                            "word": true,
                            "form": word.form,
                            "space": space,
                            "tag": word.tag.map(|code| code.to_string()),
                            "prefilled": word.prefilled,
                            "origin": word.origin.name(),
                            "guess": word.guess.map(|guess| json!({
                                "tag": guess.code.to_string(),
                                "confidence": guess.confidence.name(),
                                "others": guess.others().map(|base| base.code()).collect::<Vec<_>>(),
                            })),
                        }),
                        Row::Other(other) => json!({
                            "word": false,
                            "form": other.form,
                            "space": space,
                            "kind": kind_name(other.kind),
                        }),
                    })
                    .collect();
                json!({
                    "id": sentence.id,
                    "text": sentence.text,
                    "context": sentence.context.name(),
                    "source": note.source,
                    "repo": note.repo,
                    "reviewed": sentence.reviewed,
                    "rejected": sentence.rejected,
                    "rows": rows,
                })
            })
            .collect();
        json!({
            "file": self.file,
            "into": self.into.display().to_string(),
            "sentences": sentences,
            "guide": GUIDE,
        })
    }
}

/// Answers one connection. A connection that sends nothing, as a browser's spare one does, is
/// dropped when it times out.
fn serve<S: Store>(stream: &TcpStream, review: &Mutex<Review<S>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let response = match read_request(stream) {
        Ok(Some(request)) => review
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .handle(&request),
        Ok(None) => return,
        Err(_) => Response::error(400, "a request this server cannot read"),
    };
    let _ = write_response(stream, &response);
}

/// The request on `stream`, `None` if it closes before one arrives.
fn read_request(stream: &TcpStream) -> std::io::Result<Option<Request>> {
    let mut reader = BufReader::new(Read::take(stream, 2 * LIMIT as u64));
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    let mut parts = line.split_whitespace();
    let mut request = Request {
        method: parts.next().unwrap_or_default().to_string(),
        path: parts.next().unwrap_or_default().to_string(),
        ..Request::default()
    };
    if let Some((path, _query)) = request.path.split_once('?') {
        request.path = path.to_string();
    }
    let mut length = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header)? == 0 {
            return Ok(None);
        }
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "host" => request.host = Some(value.to_string()),
            "content-type" => request.json = value.starts_with("application/json"),
            "content-length" => length = value.parse().unwrap_or(usize::MAX),
            _ => {}
        }
    }
    if length > LIMIT {
        return Err(std::io::Error::other("the body is too large"));
    }
    request.body = vec![0; length];
    reader.read_exact(&mut request.body)?;
    Ok(Some(request))
}

fn write_response(mut stream: &TcpStream, response: &Response) -> std::io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        415 => "Unsupported Media Type",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        response.status,
        response.kind,
        response.body.len()
    )?;
    stream.write_all(response.body.as_bytes())?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::review::tests::{Memory, SKELETON};

    fn review() -> Review<Memory> {
        Review {
            session: Session::open("q.conllu", SKELETON.to_string(), "2026-10-06").unwrap(),
            store: Memory::default(),
            file: "q.conllu".into(),
            queue: "q.conllu".into(),
            into: "owner.conllu".into(),
            notes: conllu::read("q.conllu", SKELETON)
                .unwrap()
                .iter()
                .map(Note::of)
                .collect(),
            port: 8737,
        }
    }

    fn get(path: &str) -> Request {
        Request {
            method: "GET".into(),
            path: path.into(),
            host: Some("127.0.0.1:8737".into()),
            ..Request::default()
        }
    }

    fn post(path: &str, body: &Value) -> Request {
        Request {
            method: "POST".into(),
            path: path.into(),
            host: Some("localhost:8737".into()),
            json: true,
            body: body.to_string().into_bytes(),
        }
    }

    fn body(response: &Response) -> Value {
        serde_json::from_str(&response.body).unwrap()
    }

    #[test]
    fn the_page_and_the_state_are_served() {
        let mut review = review();
        let page = review.handle(&get("/"));
        assert_eq!(page.status, 200);
        assert!(page.body.contains("<html"));
        let state = body(&review.handle(&get("/state")));
        let first = &state["sentences"][0];
        assert_eq!(first["context"], "prose");
        assert_eq!(first["rows"].as_array().unwrap().len(), 7);
        assert_eq!(first["rows"][1]["word"], false);
        assert_eq!(first["rows"][1]["kind"], "Code");
        assert_eq!(
            first["rows"][2]["space"], false,
            "`now` is joined to the comma"
        );
        assert!(first["rows"][0]["guess"]["tag"].is_string());
        assert!(state["guide"].as_str().unwrap().contains("## 2. The tags"));
    }

    #[test]
    fn a_save_writes_the_sentence_as_the_review_does() {
        let mut review = review();
        let tags = json!(["V.fi", null, "R", null, "R", "V.fi", null]);
        let response = review.handle(&post("/save", &json!({ "sentence": 0, "tags": tags })));
        assert_eq!(response.status, 200, "{}", response.body);
        let saved = review.store.saved.last().unwrap();
        assert!(saved.contains("# owner_reviewed = 2026-10-06"));
        let run = saved
            .lines()
            .find(|line| line.starts_with("1\tRun"))
            .unwrap();
        assert!(
            run.contains("\tVERB\t") && run.contains("Prov=owner"),
            "{run}"
        );
        assert_eq!(
            body(&response)["state"]["sentences"][0]["reviewed"],
            "2026-10-06"
        );
    }

    #[test]
    fn a_save_with_a_word_left_blank_or_a_bad_tag_changes_nothing() {
        let mut review = review();
        for tags in [
            json!(["V.fi", null, null, null, "R", "V.fi", null]),
            json!(["V.fi", null, "N", null, "R", "V.fi", null]),
            json!(["V.fi", null, "R"]),
        ] {
            let response = review.handle(&post("/save", &json!({ "sentence": 0, "tags": tags })));
            assert_eq!(response.status, 409, "{tags}");
            assert!(body(&response)["error"].is_string());
        }
        assert!(review.store.saved.is_empty());
        assert!(review.session.sentences[0].reviewed.is_none());
    }

    #[test]
    fn a_word_left_as_deslag_filled_it_keeps_the_mark() {
        let mut review = review();
        let state = review.state();
        let rows = state["sentences"][1]["rows"].as_array().unwrap();
        let filled: Vec<usize> = (0..rows.len())
            .filter(|at| rows[*at]["prefilled"] == true)
            .collect();
        assert!(
            !filled.is_empty(),
            "deslag fills some word of `See README.md for details.`"
        );
        let tags: Vec<Value> = rows
            .iter()
            .map(|row| match row["word"].as_bool().unwrap() {
                true => row["tag"].as_str().map_or(json!("N.s"), |tag| json!(tag)),
                false => Value::Null,
            })
            .collect();
        let response = review.handle(&post("/save", &json!({ "sentence": 1, "tags": tags })));
        assert_eq!(response.status, 200, "{}", response.body);
        let saved = review.store.saved.last().unwrap();
        let block = saved.split("# sent_id = s2").nth(1).unwrap();
        let words: Vec<&str> = block.lines().filter(|l| l.contains("Kind=Word")).collect();
        for at in filled {
            assert!(words[at].ends_with("Was=prefill"), "{}", words[at]);
        }
    }

    #[test]
    fn a_word_taken_from_an_unsure_guess_is_marked_and_one_chosen_alike_is_not() {
        let mut review = review();
        let state = review.state();
        // A word deslag guesses but does not fill in, with a guess that is a whole tag.
        let (at, row, guess) = (0..2)
            .flat_map(|at| {
                let rows = state["sentences"][at]["rows"].as_array().unwrap().clone();
                rows.into_iter()
                    .enumerate()
                    .filter_map(move |(row, value)| {
                        let guess = value["guess"]["tag"].as_str()?.to_string();
                        (value["prefilled"] == false && Code::parse(&guess).is_ok())
                            .then_some((at, row, guess))
                    })
            })
            .next()
            .expect("deslag is unsure of some word of the skeleton");
        let rows = state["sentences"][at]["rows"].as_array().unwrap();
        let tags: Vec<Value> = rows
            .iter()
            .enumerate()
            .map(|(index, value)| {
                if value["word"] == false {
                    Value::Null
                } else if index == row {
                    json!(guess)
                } else {
                    value["tag"].as_str().map_or(json!("N.s"), |tag| json!(tag))
                }
            })
            .collect();
        let word_line = |review: &Review<Memory>| -> String {
            let saved = review.store.saved.last().unwrap();
            let id = format!("# sent_id = s{}", at + 1);
            let block = saved
                .split(&id)
                .nth(1)
                .unwrap()
                .split("\n\n")
                .next()
                .unwrap();
            block
                .lines()
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .nth(row)
                .unwrap()
                .to_string()
        };
        let mut accepted = vec![false; rows.len()];
        let save = json!({ "sentence": at, "tags": tags, "accepted": accepted });
        let response = review.handle(&post("/save", &save));
        assert_eq!(response.status, 200, "{}", response.body);
        assert!(
            word_line(&review).ends_with("Prov=owner"),
            "{}",
            word_line(&review)
        );
        accepted[row] = true;
        let save = json!({ "sentence": at, "tags": tags, "accepted": accepted });
        assert_eq!(review.handle(&post("/save", &save)).status, 200);
        assert!(
            word_line(&review).ends_with("Was=prefill"),
            "{}",
            word_line(&review)
        );
    }

    #[test]
    fn a_reject_is_saved_once() {
        let mut review = review();
        let response = review.handle(&post("/reject", &json!({ "sentence": 1 })));
        assert_eq!(response.status, 200, "{}", response.body);
        assert!(
            review
                .store
                .saved
                .last()
                .unwrap()
                .contains("# owner_rejected = 2026-10-06")
        );
        assert_eq!(
            review
                .handle(&post("/reject", &json!({ "sentence": 1 })))
                .status,
            409
        );
        let save = json!({ "sentence": 1, "tags": [ "V.fi", "PN.s", "P", "N.p", null ] });
        assert_eq!(review.handle(&post("/save", &save)).status, 409);
        assert_eq!(review.store.saved.len(), 1);
    }

    #[test]
    fn another_host_or_a_change_not_sent_as_json_is_refused() {
        let mut review = review();
        let mut request = get("/state");
        request.host = Some("evil.example:8737".into());
        assert_eq!(review.handle(&request).status, 403);
        request.host = None;
        assert_eq!(review.handle(&request).status, 403);
        let mut request = post("/reject", &json!({ "sentence": 0 }));
        request.json = false;
        assert_eq!(review.handle(&request).status, 415);
        assert!(review.store.saved.is_empty());
    }

    #[test]
    fn a_request_is_read_off_the_wire() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            let body = r#"{"sentence":0}"#;
            write!(
                stream,
                "POST /reject?x=1 HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\
                 Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            let mut answer = String::new();
            stream.read_to_string(&mut answer).unwrap();
            answer
        });
        let (stream, _) = listener.accept().unwrap();
        let request = read_request(&stream).unwrap().unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/reject");
        assert_eq!(
            request.host.as_deref(),
            Some(format!("127.0.0.1:{port}").as_str())
        );
        assert!(request.json);
        assert_eq!(request.body, br#"{"sentence":0}"#);
        write_response(&stream, &Response::error(404, "no")).unwrap();
        drop(stream);
        let answer = client.join().unwrap();
        assert!(answer.starts_with("HTTP/1.1 404 Not Found\r\n"), "{answer}");
        assert!(answer.ends_with(r#"{"error":"no"}"#), "{answer}");
    }
}
