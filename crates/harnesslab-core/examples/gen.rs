//! Dev tool: generate the seeded sample project's code in a target language and
//! write it to a directory, for eyeballing or compiling the output.
//!
//! Usage: cargo run -p harnesslab-core --example gen -- <python|typescript|rust> <out_dir>

use harnesslab_core::{
    factory,
    project::{AgentSpec, ComponentRef, Project, Topology},
    store::Store,
};

fn project_for(coordinator: Option<&str>) -> Project {
    match coordinator {
        None => {
            let store = Store::in_memory();
            store.seed_if_empty().expect("seed");
            store.get_project("sample").expect("sample project")
        }
        Some(coord) => {
            let names = ["manager", "worker_a", "worker_b"];
            let agents: Vec<AgentSpec> = names.iter().map(|n| AgentSpec::new(n)).collect();
            Project {
                id: "p".into(),
                name: format!("coord {coord}"),
                description: String::new(),
                agents,
                topology: Topology {
                    mode: "coordinated".into(),
                    coordinator: Some(ComponentRef::new(coord)),
                    agent_order: names.iter().map(|s| s.to_string()).collect(),
                },
                datasets: vec![],
                scorer: None,
                rev: 0,
                created_at: 0.0,
                updated_at: 0.0,
            }
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let lang = args.next().unwrap_or_else(|| "rust".to_string());
    let out = args
        .next()
        .unwrap_or_else(|| "/tmp/harnesslab-gen".to_string());
    let coordinator = args.next();

    let project = project_for(coordinator.as_deref());

    let generated = factory::generate(&project, &lang, &serde_json::Value::Null)
        .unwrap_or_else(|e| panic!("generate failed: {e}"));

    for (path, content) in &generated.files {
        let dest = std::path::Path::new(&out).join(path);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, content).unwrap();
    }
    println!("wrote {} files to {out}", generated.files.len());
}
