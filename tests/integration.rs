//! The P0 benchmark suite — docs/ANX-Implementation-Plan-v1.md Phase 7,
//! and the PRD's actual leading success metric. Every `tests/benchmarks/
//! NN_name.nx` runs through **both** `anx run` (interpreter) and a
//! built-then-executed `anx build` binary (compiled path), diffing stdout
//! against the paired `NN_name.expected` file each time.
//!
//! Expected values were hand-traced per algorithm in Phase 3, cross-checked
//! against the interpreter in Phase 4, and against the compiled path
//! manually in Phase 6 — this file is what makes that verification into a
//! standing regression suite instead of a one-time manual check.

use assert_cmd::Command;
use std::path::Path;

struct Benchmark {
    name: &'static str,
}

const BENCHMARKS: &[Benchmark] = &[
    Benchmark { name: "01_binary_search" },
    Benchmark { name: "02_binary_search_first_last" },
    Benchmark { name: "03_two_sum_sorted" },
    Benchmark { name: "04_reverse_array" },
    Benchmark { name: "05_remove_duplicates" },
    Benchmark { name: "06_bubble_sort" },
    Benchmark { name: "07_insertion_sort" },
    Benchmark { name: "08_selection_sort" },
    Benchmark { name: "09_merge_sort" },
    Benchmark { name: "10_quicksort" },
    Benchmark { name: "11_factorial" },
    Benchmark { name: "12_fibonacci_naive" },
    Benchmark { name: "13_fibonacci_memo" },
    Benchmark { name: "14_fast_exponentiation" },
    Benchmark { name: "15_gcd" },
    Benchmark { name: "16_climbing_stairs" },
    Benchmark { name: "17_coin_change" },
    Benchmark { name: "18_knapsack" },
    Benchmark { name: "19_longest_increasing_subsequence" },
    Benchmark { name: "20_max_subarray" },
];

fn benchmarks_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/benchmarks")
}

fn read_expected(name: &str) -> String {
    std::fs::read_to_string(benchmarks_dir().join(format!("{name}.expected")))
        .unwrap_or_else(|e| panic!("missing/unreadable {name}.expected: {e}"))
}

fn run_via_interpreter(name: &str) -> String {
    let source = benchmarks_dir().join(format!("{name}.nx"));
    let output = Command::cargo_bin("anx")
        .unwrap()
        .arg("run")
        .arg(&source)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{name}: `anx run` failed (exit {:?}): {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn run_via_compiled_binary(name: &str) -> String {
    let source = benchmarks_dir().join(format!("{name}.nx"));
    let bin_path = std::env::temp_dir().join(format!("anx-benchmark-{name}"));

    let build = Command::cargo_bin("anx")
        .unwrap()
        .arg("build")
        .arg(&source)
        .arg("-o")
        .arg(&bin_path)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{name}: `anx build` failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let run = std::process::Command::new(&bin_path)
        .output()
        .unwrap_or_else(|e| panic!("{name}: compiled binary failed to execute: {e}"));
    assert!(
        run.status.success(),
        "{name}: compiled binary exited with {:?}: {}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );

    let _ = std::fs::remove_file(&bin_path);
    String::from_utf8(run.stdout).unwrap()
}

#[test]
fn all_benchmarks_pass_on_both_the_interpreter_and_compiled_paths() {
    let mut failures = Vec::new();

    for b in BENCHMARKS {
        let expected = read_expected(b.name);

        let interp_out = run_via_interpreter(b.name);
        if interp_out != expected {
            failures.push(format!(
                "{}: interpreter output mismatch\n  expected: {expected:?}\n  actual:   {interp_out:?}",
                b.name
            ));
        }

        let compiled_out = run_via_compiled_binary(b.name);
        if compiled_out != expected {
            failures.push(format!(
                "{}: compiled output mismatch\n  expected: {expected:?}\n  actual:   {compiled_out:?}",
                b.name
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{}/{} benchmark checks failed:\n\n{}",
        failures.len(),
        BENCHMARKS.len() * 2,
        failures.join("\n\n")
    );
}
