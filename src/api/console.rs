//! The console bundle the brain serves (docs/boundary-api.md, 「The console bundle」): its manifest, its module
//! and stylesheet under names that follow their bytes, its words, and how far the console's token reaches.

use std::io;
use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Json, Response};
use bmo_brain_client::{ConsoleManifest, ErrorCode, Role, SidebarSection};
use sha2::{Digest, Sha256};

use super::{Answer, ApiError, Brain, Key};

/// The least role a reader holds to open the screen, and so the floor of every route the screen uses.
pub const SCREEN_ROLE: Role = Role::Administrator;

/// The bundle's name, its words' namespace, its screen path and the service's relay name.
const NAME: &str = "brain";
/// The path the module, the stylesheet and the words are answered under.
const FILES: &str = "/console-bundle";

/// The files the build of `client/` writes into the bundle folder.
const MODULE_FILE: &str = "module.js";
const STYLESHEET_FILE: &str = "style.css";
const WORDS_FILE: &str = "words.json";
/// The file bmo-ui's bundle build writes beside the module, naming the layer the bundle was built on.
const LAYER_FILE: &str = "layer.json";

/// How many hex digits of a file's SHA-256 its name carries.
const HASH_DIGITS: usize = 16;

/// How far the console's token reaches a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
  /// With no role: the manifest and the words, which a console reads for no reader.
  Open,
  /// With an `Owner-Role` allowing the screen's role: the module, the stylesheet and every route the screen
  /// uses.
  Screen,
  /// Not at all.
  Closed,
}

impl Reach {
  /// The reach as the OpenAPI document's `x-console` writes it.
  pub fn name(self) -> &'static str {
    match self {
      Self::Open => "open",
      Self::Screen => "screen",
      Self::Closed => "closed",
    }
  }
}

/// The routes the screen uses, by method and path as the router writes them.
const SCREEN_ROUTES: [(&str, &str); 27] = [
  ("get", "/events"),
  ("get", "/console-bundle/{file}"),
  ("get", "/brains"),
  ("get", "/brains/{brainId}"),
  ("get", "/brains/{brainId}/subjects"),
  ("post", "/brains/{brainId}/subjects"),
  ("get", "/brains/{brainId}/subjects/{subjectId}"),
  ("patch", "/brains/{brainId}/subjects/{subjectId}"),
  ("get", "/brains/{brainId}/subjects/{subjectId}/sheet"),
  ("get", "/brains/{brainId}/layers"),
  ("post", "/brains/{brainId}/layers"),
  ("get", "/brains/{brainId}/layers/{layerId}"),
  ("patch", "/brains/{brainId}/layers/{layerId}"),
  ("get", "/brains/{brainId}/memories"),
  ("post", "/brains/{brainId}/memories"),
  ("get", "/brains/{brainId}/memories/{memoryId}"),
  ("patch", "/brains/{brainId}/memories/{memoryId}"),
  ("post", "/brains/{brainId}/memories/{memoryId}/versions"),
  ("post", "/brains/{brainId}/memories/{memoryId}/restorations"),
  ("get", "/brains/{brainId}/lineages/{lineageId}"),
  ("post", "/brains/{brainId}/confirmations"),
  ("get", "/brains/{brainId}/types"),
  ("get", "/brains/{brainId}/tallies"),
  ("post", "/brains/{brainId}/learnings"),
  ("post", "/brains/{brainId}/similar"),
  ("get", "/brains/{brainId}/persona"),
  ("get", "/brains/{brainId}/export"),
];

/// Whether the route at this method and path is the console bundle's own, which the console's token alone
/// reaches: the manifest, the words, the module and the stylesheet.
pub fn console_only(method: &str, path: &str) -> bool {
  method.eq_ignore_ascii_case("get")
    && matches!(path, "/console-manifest" | "/console-bundle/words" | "/console-bundle/{file}")
}

/// How far the console's token reaches the route at this method and path.
pub fn reach(method: &str, path: &str) -> Reach {
  let method = method.to_ascii_lowercase();
  match (method.as_str(), path) {
    ("get", "/console-manifest" | "/console-bundle/words") => Reach::Open,
    (method, path) if SCREEN_ROUTES.contains(&(method, path)) => Reach::Screen,
    _ => Reach::Closed,
  }
}

/// A file of the bundle as it is answered: its name under `/console-bundle/` and its bytes.
#[derive(Debug, Clone)]
pub struct Served {
  pub name: String,
  pub bytes: Vec<u8>,
}

impl Served {
  /// A file named by its stem, the first hex digits of its bytes' SHA-256 and its ending.
  fn named(stem: &str, ending: &str, bytes: Vec<u8>) -> Self {
    let hash: String = Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>()
      [..HASH_DIGITS]
      .to_owned();
    Self {
      name: format!("{stem}.{hash}.{ending}"),
      bytes,
    }
  }

  /// The path of the service's API it is answered at.
  pub fn path(&self) -> String {
    format!("{FILES}/{}", self.name)
  }
}

/// The built bundle: its module, its stylesheet where it has one, its words and the layer it was built on.
#[derive(Debug, Clone)]
pub struct Bundle {
  pub module: Served,
  pub stylesheet: Option<Served>,
  /// Each locale's tree under the locale's code, as the words file holds it.
  pub words: Vec<u8>,
  /// The version of bmo-ui the bundle was built on.
  pub layer: String,
}

/// The layer file as the bundle build writes it.
#[derive(serde::Deserialize)]
struct LayerFile {
  layer: String,
}

/// A file of the folder, or none where it does not stand.
fn read_if_standing(path: &Path) -> io::Result<Option<Vec<u8>>> {
  match std::fs::read(path) {
    Ok(bytes) => Ok(Some(bytes)),
    Err(why) if why.kind() == io::ErrorKind::NotFound => Ok(None),
    Err(why) => Err(why),
  }
}

impl Bundle {
  /// The bundle the folder holds; none where it holds no module, no words or no layer.
  pub fn read(folder: &Path) -> io::Result<Option<Self>> {
    let (Some(module), Some(words), Some(layer)) = (
      read_if_standing(&folder.join(MODULE_FILE))?,
      read_if_standing(&folder.join(WORDS_FILE))?,
      read_if_standing(&folder.join(LAYER_FILE))?,
    ) else {
      return Ok(None);
    };
    serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&words).map_err(|why| {
      io::Error::new(io::ErrorKind::InvalidData, format!("the words are no JSON object: {why}"))
    })?;
    let LayerFile { layer } = serde_json::from_slice(&layer).map_err(|why| {
      io::Error::new(io::ErrorKind::InvalidData, format!("the layer file does not name a layer: {why}"))
    })?;
    Ok(Some(Self {
      module: Served::named("module", "js", module),
      stylesheet: read_if_standing(&folder.join(STYLESHEET_FILE))?
        .map(|bytes| Served::named("style", "css", bytes)),
      words,
      layer,
    }))
  }

  /// The manifest naming this bundle, and the other services its screen calls as the settings name them.
  pub fn manifest(&self, calls: &[String]) -> ConsoleManifest {
    ConsoleManifest {
      name: NAME.to_owned(),
      title: format!("{NAME}.title.screen"),
      icon: "i-material-symbols-neurology-outline".to_owned(),
      sidebar_section: SidebarSection::Administration,
      screen_path: format!("/{NAME}"),
      service_name: NAME.to_owned(),
      layer: self.layer.clone(),
      bundle: self.module.path(),
      stylesheet: self.stylesheet.as_ref().map(Served::path),
      words: format!("{FILES}/words"),
      api_prefix: "/".to_owned(),
      role: SCREEN_ROLE,
      owner_key: false,
      events: true,
      websocket: false,
      push_kinds: vec![],
      calls: calls.to_vec(),
    }
  }

  /// The module or the stylesheet the name names, with its type.
  fn file(&self, name: &str) -> Option<(&Served, &'static str)> {
    let module = (&self.module, "text/javascript; charset=utf-8");
    let stylesheet = self.stylesheet.as_ref().map(|served| (served, "text/css; charset=utf-8"));
    [Some(module), stylesheet].into_iter().flatten().find(|(served, _)| served.name == name)
  }
}

/// The console the brain serves a bundle to: the token it calls with, the bundle where one was built, and the
/// other services the screen calls through the console.
#[derive(Debug, Clone)]
pub struct Console {
  pub token: String,
  pub bundle: Option<Bundle>,
  pub calls: Vec<String>,
}

impl Console {
  /// The console over its token, its bundle read from the folder; a bundle that does not read is told on the
  /// error stream and left out, so the service stands without a screen.
  pub fn over(token: String, folder: &Path, calls: Vec<String>) -> Self {
    let bundle = Bundle::read(folder).unwrap_or_else(|why| {
      eprintln!("bmo-brain: the console bundle in {} could not be read: {why}", folder.display());
      None
    });
    Self {
      token,
      bundle,
      calls,
    }
  }
}

/// The folder a release's bundle stands in: `console` in the directory above the one holding the binary.
pub fn bundle_beside_binary() -> Option<PathBuf> {
  let binary = std::env::current_exe().ok()?;
  Some(binary.parent()?.parent()?.join("console"))
}

/// The bundle the brain stands with, or the refusal a console reads as a service with no screen.
fn bundle_of(brain: &Brain) -> Answer<&Bundle> {
  brain
    .console
    .as_ref()
    .and_then(|console| console.bundle.as_ref())
    .ok_or_else(|| ApiError::new(ErrorCode::NotFound, "The brain holds no console bundle."))
}

/// An answer of bytes under its headers.
fn bytes(body: Vec<u8>, headers: &[(header::HeaderName, &'static str)]) -> Response {
  let mut response = (StatusCode::OK, body).into_response();
  for (name, value) in headers {
    response.headers_mut().insert(name.clone(), HeaderValue::from_static(value));
  }
  response
}

/// `GET /console-manifest`
pub(super) async fn manifest(State(brain): State<Brain>) -> Answer<Json<ConsoleManifest>> {
  let calls = brain.console.as_ref().map(|console| console.calls.as_slice()).unwrap_or_default();
  Ok(Json(bundle_of(&brain)?.manifest(calls)))
}

/// `GET /console-bundle/words`
pub(super) async fn words(State(brain): State<Brain>) -> Answer<Response> {
  let words = bundle_of(&brain)?.words.clone();
  Ok(bytes(
    words,
    &[(header::CONTENT_TYPE, "application/json"), (header::CACHE_CONTROL, "no-cache")],
  ))
}

/// `GET /console-bundle/{file}`
pub(super) async fn file(State(brain): State<Brain>, Key(name): Key<String>) -> Answer<Response> {
  let (served, kind) = bundle_of(&brain)?
    .file(&name)
    .ok_or_else(|| ApiError::new(ErrorCode::NotFound, format!("The console bundle has no file {name}.")))?;
  Ok(bytes(
    served.bytes.clone(),
    &[
      (header::CONTENT_TYPE, kind),
      (header::CACHE_CONTROL, "private, max-age=31536000, immutable"),
      (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ],
  ))
}
