//! Main entry point for the `tsp-ty` binary.
//!
//! This binary launches the TSP-enabled ty type server, which can be used
//! with Pylance via the `python.analysis.typeServerExecutable` setting.

fn main() -> anyhow::Result<()> {
    tsp_ty_server::run_server()
}
