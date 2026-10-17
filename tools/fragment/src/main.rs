//! The fragment tool's executable: one call handed by the tool host on standard input, answered on standard output
//! (docs/boundary-api.md, 「Its tools」).

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::process::ExitCode {
  tool_call::answer_standard_io(fragment_tool::answer).await
}
