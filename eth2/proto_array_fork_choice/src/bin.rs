mod error;
mod fork_choice_test_definition;
mod proto_array;
mod proto_array_fork_choice;
mod ssz_container;

pub use fork_choice_test_definition::*;
use serde_yaml;
use std::fs::File;

fn main() {
    write_test_def_to_yaml("votes.yaml", get_votes_test_definition());
    write_test_def_to_yaml("no_votes.yaml", get_no_votes_test_definition());
}

fn write_test_def_to_yaml(filename: &str, def: ForkChoiceTestDefinition) {
    let file = File::create(filename).expect("Should be able to open file");
    serde_yaml::to_writer(file, &def).expect("Should be able to write YAML to file");
}
