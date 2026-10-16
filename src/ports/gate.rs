//! The gate port: the document sanitiser and the sheet judgement (`docs/boundary-api.md`, 「The gate」).

use futures_util::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::ports::http::{Caller, Peer, PortFailure, Refusal};

/// What the sanitiser made of a document.
#[derive(Debug, Clone, PartialEq)]
pub enum Sanitised {
  /// The text to keep, changed or not.
  Text(String),
  /// The sanitiser would not answer, and why.
  Refused(Refusal),
}

/// Which part of a person's book a sheet is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SheetPart {
  Common,
  Room,
}

/// One piece a sheet is written from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Piece {
  pub path: String,
  pub text: String,
}

/// A sheet to judge: one person's pieces of one part, and the length the sheet may not pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetAsk {
  pub agent: String,
  pub part: SheetPart,
  /// The room's, for the room part; none for the common part.
  pub chat_id: Option<i64>,
  pub person: i64,
  pub pieces: Vec<Piece>,
  pub ceiling: i64,
  pub registration: Option<String>,
}

/// What the brain asks of the gate.
pub trait Gate: Send + Sync {
  /// The sanitiser's reading of a document the brain is about to keep.
  fn sanitise<'a>(&'a self, path: &'a str, text: &'a str) -> BoxFuture<'a, Result<Sanitised, PortFailure>>;

  /// The sheet the judgement wrote, or none where none came.
  fn judge_sheet<'a>(&'a self, ask: &'a SheetAsk) -> BoxFuture<'a, Result<Option<String>, PortFailure>>;
}

/// The gate over HTTP.
#[derive(Debug, Clone)]
pub struct HttpGate {
  caller: Caller,
}

impl HttpGate {
  pub fn new(peer: &Peer) -> Self {
    Self::with_caller(Caller::new(peer))
  }

  pub fn with_caller(caller: Caller) -> Self {
    Self { caller }
  }
}

#[derive(Serialize)]
struct DocumentAsk<'a> {
  path: &'a str,
  text: &'a str,
}

#[derive(Deserialize)]
struct SanitiserAnswer {
  answered: bool,
  text: Option<String>,
  refusal: Option<Refusal>,
}

#[derive(Deserialize)]
struct Sheet {
  sheet: Option<String>,
}

impl Gate for HttpGate {
  fn sanitise<'a>(&'a self, path: &'a str, text: &'a str) -> BoxFuture<'a, Result<Sanitised, PortFailure>> {
    Box::pin(async move {
      let route = "/sanitiser/documents";
      let answer: SanitiserAnswer = self.caller.post(route, &DocumentAsk { path, text }).await?;
      match answer {
        SanitiserAnswer {
          answered: true,
          text: Some(text),
          ..
        } => Ok(Sanitised::Text(text)),
        SanitiserAnswer {
          answered: false,
          refusal: Some(refusal),
          ..
        } => Ok(Sanitised::Refused(refusal)),
        SanitiserAnswer { answered, .. } => Err(PortFailure::Undecodable {
          url: self.caller.url(route),
          why: format!("an answer with answered {answered} carries neither its text nor its refusal"),
        }),
      }
    })
  }

  fn judge_sheet<'a>(&'a self, ask: &'a SheetAsk) -> BoxFuture<'a, Result<Option<String>, PortFailure>> {
    Box::pin(async move {
      let answer: Sheet = self.caller.post("/sheets/judge", ask).await?;
      Ok(answer.sheet)
    })
  }
}
