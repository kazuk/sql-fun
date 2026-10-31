//! Query-scoped memoization of local alternative compatibility.
//!
//! Only owned immutable values are retained. Node labels are normalized to
//! alias positions, while current domains are interned by complete equality.
use crate::sem::parameter_constraints::{
    InputTypeConstraint, OverloadConstraint, SignatureAlternative,
};
use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct ConstraintMemo {
    domains: HashMap<InputTypeConstraint, usize>,
    compatibility: HashMap<SignatureAlternative, HashMap<OperandDomains, bool>>,
}

impl OverloadConstraint {
    // Capture current domains once per overload, not once per alternative.
    // The IDs belong to this memo and are consumed before graph mutation.
    pub(super) fn operand_domains(
        &self,
        domains: &[InputTypeConstraint],
        memo: &mut ConstraintMemo,
    ) -> OperandDomains {
        let mut current = Vec::with_capacity(self.nodes.len());
        let mut aliases = Vec::with_capacity(self.nodes.len());
        for (index, node) in self.nodes.iter().enumerate() {
            current.push(memo.domain_id(&domains[*node]));
            aliases.push(
                self.nodes[..index]
                    .iter()
                    .position(|previous| previous == node)
                    .unwrap_or(index),
            );
        }
        OperandDomains { aliases, current }
    }
}

#[cfg(test)]
mod test_operand_domains {
    use crate::sem::parameter_constraints::{
        InputTypeConstraint, OverloadConstraint, SignatureAlternative,
    };
    use crate::sem::{ConstraintMemo, PgBuiltInType};
    #[rstest::rstest]
    #[case::independent([0, 1], true)]
    #[case::same_operand([0, 0], false)]
    #[case::reversed([1, 0], false)]
    fn compatibility_preserves_operand_identity(#[case] nodes: [usize; 2], #[case] expected: bool) {
        let int = InputTypeConstraint::exact(&PgBuiltInType::int4());
        let text = InputTypeConstraint::exact(&PgBuiltInType::text());
        let domains = vec![int.clone(), text.clone(), int.clone(), text.clone()];
        let alternative = SignatureAlternative::new(&[int, text]);
        let mut memo = ConstraintMemo::default();
        // Renumbering another graph preserves aliases, including repeated operands.
        for graph_nodes in [nodes, [nodes[0] + 2, nodes[1] + 2], nodes] {
            let overload = OverloadConstraint::new(
                &graph_nodes,
                std::slice::from_ref(&alternative),
                "operands",
            );
            let operands = overload.operand_domains(&domains, &mut memo);
            assert_eq!(
                memo.compatible(&alternative, &operands, &graph_nodes, &domains),
                expected
            );
        }
    }
}

impl ConstraintMemo {
    fn domain_id(&mut self, domain: &InputTypeConstraint) -> usize {
        if let Some(id) = self.domains.get(domain) {
            return *id;
        }
        let id = self.domains.len();
        self.domains.insert(domain.clone(), id);
        id
    }

    pub(super) fn compatible(
        &mut self,
        alternative: &SignatureAlternative,
        operands: &OperandDomains,
        nodes: &[usize],
        domains: &[InputTypeConstraint],
    ) -> bool {
        if let Some(entries) = self.compatibility.get(alternative)
            && let Some(result) = entries.get(operands)
        {
            return *result;
        }
        let result = alternative.compatible_with(nodes, domains);
        if let Some(entries) = self.compatibility.get_mut(alternative) {
            entries.insert(operands.clone(), result);
        } else {
            self.compatibility.insert(
                alternative.clone(),
                HashMap::from([(operands.clone(), result)]),
            );
        }
        result
    }
}

#[cfg(test)]
mod test_compatibility {
    use crate::sem::parameter_constraints::{
        InputTypeConstraint, OverloadConstraint, SignatureAlternative,
    };
    use crate::sem::{ConstraintMemo, PgBuiltInType};
    #[test]
    fn compatibility_observes_domain_mutation_and_exclusions() {
        let int = InputTypeConstraint::exact(&PgBuiltInType::int4());
        let text = InputTypeConstraint::exact(&PgBuiltInType::text());
        let required = SignatureAlternative::new(&[int.clone(), text.clone()]);
        let excluded = SignatureAlternative::excluding(
            &[InputTypeConstraint::any(), text.clone()],
            &[vec![PgBuiltInType::int4()]],
        );
        let mut memo = ConstraintMemo::default();
        let nodes = [0, 1];
        let mut domains = vec![int, text];
        let overload =
            OverloadConstraint::new(&nodes, &[required.clone(), excluded.clone()], "mutation");
        let first = overload.operand_domains(&domains, &mut memo);
        assert!(memo.compatible(&required, &first, &nodes, &domains));
        domains.swap(0, 1);
        let swapped = overload.operand_domains(&domains, &mut memo);
        assert!(!memo.compatible(&required, &swapped, &nodes, &domains));
        domains.swap(0, 1);
        let restored = overload.operand_domains(&domains, &mut memo);
        assert!(memo.compatible(&required, &restored, &nodes, &domains));
        assert!(!memo.compatible(&excluded, &restored, &nodes, &domains));
        assert!(memo.compatible(&required, &restored, &nodes, &domains));
        assert!(!memo.compatible(&excluded, &restored, &nodes, &domains));
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub(super) struct OperandDomains {
    aliases: Vec<usize>,
    current: Vec<usize>,
}
