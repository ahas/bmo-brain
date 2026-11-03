//! The memory tool's executable: one call handed by the tool host on standard input, answered on standard
//! output (docs/boundary-api.md, 「The memory tool」).

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
  memory_tool::answer_standard_io().await
}
