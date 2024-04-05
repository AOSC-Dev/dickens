pub mod sodep;
pub mod topic;

pub fn escape_name_for_graphviz(name: &str) -> String {
    name.replace("-", "_dash_").replace("+", "_plus_")
}
