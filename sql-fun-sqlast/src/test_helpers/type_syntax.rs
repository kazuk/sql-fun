use crate::syn::{Node, NodeInner, NodeListOpt, String as SynString};

pub(crate) fn build_string_node_list_opt(strings: &[&str]) -> NodeListOpt {
    use crate::syn::StringBuilder;

    let mut string_values = Vec::new();
    for string in strings {
        let string_value = StringBuilder::default()
            .sval(String::from(*string))
            .build()
            .unwrap();
        let string_value = SynString::from(string_value);
        let string_node = NodeInner::from(string_value);
        let string_node = Node::from(string_node);
        string_values.push(string_node);
    }
    NodeListOpt::from(string_values)
}
