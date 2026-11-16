# The tests' databases

A test that reaches the store does so over a PostgreSQL database of its own on the suite's server, the one
`BMO_TEST_POSTGRES` names, else the local socket's `bmo_test`. The service's harness and the tool's harness
make them the same way, through one module of the service's test support.

- **A template a binary, a copy a test.** The first test of a binary that asks makes a template and runs the
  migrations into it; each test is handed a copy of it, or a blank database where it runs only the migrations
  it reads past. A template is named `bmo_brain_test_tmpl_` followed by the binary's process id, every other
  database `bmo_brain_test_copy_` followed by the process id and a count.
- **A test's database is dropped when the test ends, and the test fails if it cannot be.** The drop first ends
  the suite role's own sessions on the database and then drops it without forcing: forcing needs the right to
  end other roles' processes, an autovacuum worker's among them, which the suite's role need not hold, while
  the plain drop stops an autovacuum worker itself. A session that opens between the two is ended on the next
  of a few attempts.
- **What a binary that is gone left behind is swept.** Before a binary makes its first database it drops every
  database of these two names whose process no longer runs on this machine and that no client's session is
  on; the template, which lives as long as its binary, is dropped this way by the next run. The sweep also
  reads `brain_rs_tmpl_`, `brain_rs_copy_` and `brain_tool_`, the names a checkout behind this rule still
  makes its databases under, and no other name, so another repository's databases on the same server are
  never touched. A database it cannot drop is named on the error stream and left for the next run.
