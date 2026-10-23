//! The brain tool's executable: one call handed by the tool host on standard input, answered on standard
//! output (docs/boundary-api.md, 「Its tool」).

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
  brain_tool::answer_standard_io().await
}
