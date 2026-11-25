//! The harness plugin's MCP server (docs/boundary-api.md, 「The harness plugin」): the brain's tools over one
//! service, each call made through the brain's client.

mod tools;

use std::sync::Arc;

use bmo_brain_client::BrainClient;
use rmcp::model::{
  CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation, InitializeResult,
  JsonObject, ListToolsResult, PaginatedRequestParams, ServerCapabilities, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler};
use schemars::Schema;
use schemars::generate::SchemaSettings;
use serde_json::{Map, Value};

pub use tools::{Answered, BrainTool, ToolFailure, tools};

/// What the server tells a harness of itself when it starts.
const INSTRUCTIONS: &str = "Keeps an agent's long-term memory as brains, one an account's and one \
  agent's. Find or make the brain with brain_brains and brain_brain_add, read what a turn should know with \
  brain_recall, judge what is learned with brain_similar and apply it with brain_learn, write with \
  brain_memory_write and brain_memory_version, and close a memory with brain_memory_move. A failure comes back \
  as a tool error carrying the route's status, code and sentence.";

/// The server over the brain the client reaches.
pub struct BrainServer {
  client: BrainClient,
  tools: Arc<Vec<BrainTool>>,
}

impl BrainServer {
  pub fn new(client: BrainClient) -> Self {
    Self {
      client,
      tools: Arc::new(tools()),
    }
  }

  /// The tools as MCP lists them, each input and output a schema of its own with its definitions inside.
  pub fn listed(&self) -> Vec<Tool> {
    self.tools.iter().map(listed).collect()
  }
}

fn listed(tool: &BrainTool) -> Tool {
  let mut generator = SchemaSettings::draft2020_12()
    .for_serialize()
    .with(|settings| settings.meta_schema = None)
    .into_generator();
  let input = tool.input(&mut generator);
  let output = tool.output(&mut generator);
  let definitions = generator.take_definitions(true);
  let mut listed = Tool::new(tool.name, tool.description, standing_alone(input, &definitions));
  listed.output_schema = Some(Arc::new(standing_alone(output, &definitions)));
  listed
}

/// A schema made to stand on its own: an object at its root, with every definition it refers to beside it.
fn standing_alone(schema: Schema, definitions: &Map<String, Value>) -> JsonObject {
  let mut object = match schema.to_value() {
    Value::Object(object) if object.contains_key("$ref") => {
      let mut wrapped = Map::new();
      wrapped.insert("allOf".into(), Value::Array(vec![Value::Object(object)]));
      wrapped.insert("type".into(), "object".into());
      wrapped
    }
    Value::Object(object) => object,
    _ => unreachable!("every tool's input and output is an object"),
  };
  if !definitions.is_empty() {
    object.insert("$defs".into(), Value::Object(definitions.clone()));
  }
  object
}

impl ServerHandler for BrainServer {
  fn get_info(&self) -> InitializeResult {
    let mut info = InitializeResult::new(ServerCapabilities::builder().enable_tools().build());
    let mut server = Implementation::default();
    server.name = "bmo-brain".to_owned();
    server.version = crate::VERSION.to_owned();
    info.server_info = server;
    info.instructions = Some(INSTRUCTIONS.to_owned());
    info
  }

  async fn list_tools(
    &self,
    _: Option<PaginatedRequestParams>,
    _: RequestContext<RoleServer>,
  ) -> Result<ListToolsResult, ErrorData> {
    Ok(ListToolsResult::with_all_items(self.listed()))
  }

  async fn call_tool(
    &self,
    request: CallToolRequestParams,
    _: RequestContext<RoleServer>,
  ) -> Result<CallToolResponse, ErrorData> {
    let Some(tool) = self.tools.iter().find(|tool| tool.name == request.name) else {
      return Err(ErrorData::invalid_params(format!("There is no tool {}.", request.name), None));
    };
    let arguments = Value::Object(request.arguments.unwrap_or_default());
    let result = match tool.call(self.client.clone(), arguments).await {
      Ok(answer) => CallToolResult::structured(answer),
      Err(failure) => failed(&failure),
    };
    Ok(CallToolResponse::Complete(result))
  }
}

/// A failed call as a tool error whose text is the failure's content. It carries no structured content, which
/// would be read against the tool's output schema.
fn failed(failure: &ToolFailure) -> CallToolResult {
  CallToolResult::error(vec![ContentBlock::text(failure.content().to_string())])
}
