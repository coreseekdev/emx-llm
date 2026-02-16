//! E2E tests for emx-gate
//!
//! End-to-end tests using emx-testspec framework

use emx_testspec::{RunConfig, TestRunner};

fn run_e2e_tests(filter: Option<String>) {
    let emx_llm_path = std::path::PathBuf::from("/s/src.editor/workspace/emx-llm/target/debug");

    let config = RunConfig {
        dir: "tests/e2e".into(),
        filter,
        workdir_root: None,
        preserve_work: false,
        verbose: std::env::var("E2E_VERBOSE").is_ok(),
        extensions: vec![".txtar".into()],
        setup: Some(Box::new(move |setup_env: &mut emx_testspec::SetupEnv| {
            let current_path = std::env::var("PATH").unwrap_or_default();
            setup_env.env.push((
                "PATH".to_string(),
                format!("{}:{}", emx_llm_path.display(), current_path),
            ));
            Ok(())
        })),
    };

    let runner = TestRunner::new(config);
    let result = runner.run_all().expect("Failed to run E2E tests");

    println!("\n=== E2E Test Summary ===");
    println!("Total: {}", result.cases.len());
    println!("Passed: {}", result.passed_count());
    println!("Failed: {}", result.failed_count());

    for case in &result.cases {
        if !case.passed && !case.skipped {
            println!("\n  - {} (error: {:?})", case.name, case.error);
            if result.cases.len() == 1 || std::env::var("E2E_VERBOSE").is_ok() {
                println!("\n--- Log ---");
                println!("{}", case.log);
            }
        }
    }

    assert!(result.all_passed(), "Some E2E tests failed");
}

#[test]
fn test_e2e_health_check() {
    run_e2e_tests(Some("001".to_string()));
}

#[test]
fn test_e2e_openai_endpoint() {
    run_e2e_tests(Some("002".to_string()));
}

#[test]
fn test_e2e_anthropic_endpoint() {
    run_e2e_tests(Some("003".to_string()));
}

#[test]
fn test_e2e_list_endpoints() {
    run_e2e_tests(Some("004".to_string()));
}

#[test]
fn test_e2e_error_handling() {
    run_e2e_tests(Some("005".to_string()));
}
