//! E2E tests for emx-gate
//!
//! End-to-end tests using emx-testspec framework

use emx_testspec::{RunConfig, TestRunner};

/// Run all E2E tests
fn run_e2e_tests() {
    let config = RunConfig {
        dir: "tests/e2e".into(),
        filter: None,
        workdir_root: None,
        preserve_work: false,
        verbose: std::env::var("E2E_VERBOSE").is_ok(),
        extensions: vec![".txtar".into()],
        setup: None,
    };

    let runner = TestRunner::new(config);
    let result = runner.run_all().expect("Failed to run E2E tests");

    // Print summary
    println!("\n=== E2E Test Summary ===");
    println!("Total: {}", result.total());
    println!("Passed: {}", result.passed());
    println!("Failed: {}", result.failed());

    if let Some(failed) = result.failed_details() {
        for test in failed {
            println!("  - {}", test);
        }
    }

    assert!(result.all_passed(), "Some E2E tests failed");
}

#[test]
fn test_e2e_health_check() {
    run_e2e_tests();
}

#[test]
fn test_e2e_openai_endpoint() {
    run_e2e_tests();
}

#[test]
fn test_e2e_anthropic_endpoint() {
    run_e2e_tests();
}

#[test]
fn test_e2e_list_endpoints() {
    run_e2e_tests();
}

#[test]
fn test_e2e_error_handling() {
    run_e2e_tests();
}
