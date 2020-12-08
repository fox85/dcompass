// Copyright 2020 LEXUGE
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

/// Structures used for serialize/deserialize information needed to create router and more.
pub mod parsed;
pub mod rule;

use self::{
    parsed::ParsedRule,
    rule::{actions::ActionError, matchers::MatchError, Rule},
};
use super::upstreams::Upstreams;
use crate::Label;
use hashbrown::{HashMap, HashSet};
use log::*;
use thiserror::Error;
use trust_dns_client::op::Message;

type Result<T> = std::result::Result<T, TableError>;

/// Errors generated by the `table` section.
#[derive(Error, Debug)]
pub enum TableError {
    /// Errors related to matchers.
    #[error(transparent)]
    MatchError(#[from] MatchError),

    /// Errors related to actions
    #[error(transparent)]
    ActionError(#[from] ActionError),

    /// Rules are defined recursively, which is prohibited.
    #[error("The `rule` block with tag `{0}` is being recursively called in the `table` section")]
    RuleRecursion(Label),

    /// A rule is not found.
    #[error(
        "Rule with tag `{0}` is not found in the `table` section. Note that tag `start` is required"
    )]
    UndefinedTag(Label),

    /// Multiple rules with the same tag name have been found.
    #[error("Multiple defintions found for tag `{0}` in the `rules` section")]
    MultipleDef(Label),
}

pub struct State {
    resp: Message,
    query: Message,
}

/// A simple routing table.
pub struct Table {
    rules: HashMap<Label, Rule>,
    used: HashSet<Label>,
}

impl Table {
    /// Create a routing table from a bunch of `Rule`s.
    pub fn new(rules: Vec<Rule>) -> Result<Self> {
        let mut table = HashMap::new();
        for r in rules {
            match table.get(r.tag()) {
                Some(_) => return Err(TableError::MultipleDef(r.tag().clone())),
                None => table.insert(r.tag().clone(), r),
            };
        }
        let mut used = HashSet::new();
        Self::traverse(&table, &mut HashSet::new(), &mut used, &"start".into())?;
        Ok(Self { rules: table, used })
    }

    // This is not intended to be used by end-users as they can create with parsed structs from `Router`.
    pub(super) async fn with_parsed(parsed_rules: Vec<ParsedRule>) -> Result<Self> {
        let mut rules = Vec::new();
        for r in parsed_rules {
            rules.push(Rule::with_parsed(r).await?);
        }
        Self::new(rules)
    }

    // Not intended to be used by end-users
    pub(super) fn used(&self) -> &HashSet<Label> {
        &self.used
    }

    fn traverse(
        rules: &HashMap<Label, Rule>,
        l: &mut HashSet<Label>,
        used: &mut HashSet<Label>,
        tag: &Label,
    ) -> Result<()> {
        if let Some(r) = rules.get(tag) {
            if l.contains(tag) {
                Err(TableError::RuleRecursion(tag.clone()))
            } else {
                l.insert(tag.clone());
                if r.on_match_next() != &"end".into() {
                    Self::traverse(rules, l, used, r.on_match_next())?;
                }
                if r.no_match_next() != &"end".into() {
                    Self::traverse(rules, l, used, r.no_match_next())?;
                }
                used.extend(r.used_upstreams());
                Ok(())
            }
        } else {
            Err(TableError::UndefinedTag(tag.clone()))
        }
    }

    // Not intended to be used by end-users
    pub(super) async fn route(&self, query: Message, upstreams: &Upstreams) -> Result<Message> {
        let name = query.queries().iter().next().unwrap().name().to_utf8();
        let mut s = State {
            resp: Message::new(),
            query,
        };

        let mut tag = "start".into();
        while tag != "end".into() {
            tag = self
                .rules
                .get(&tag)
                .unwrap()
                .route(&mut s, upstreams, &name, &tag)
                .await?;
        }
        info!("Domain \"{}\" has finished routing", name);
        Ok(s.resp)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        rule::{
            actions::{Query, Skip},
            matchers::{Any, Domain},
            Rule,
        },
        Table, TableError,
    };

    #[tokio::test]
    async fn fail_table_recursion() {
        match Table::new(vec![Rule::new(
            "start".into(),
            Box::new(Any::default()),
            (Box::new(Query::new("mock".into())), "end".into()),
            (Box::new(Skip::default()), "start".into()),
        )])
        .err()
        .unwrap()
        {
            TableError::RuleRecursion(_) => {}
            e => panic!("Not the right error type: {}", e),
        }
    }

    #[tokio::test]
    async fn fail_multiple_defs() {
        match Table::new(vec![
            Rule::new(
                "start".into(),
                Box::new(Any::default()),
                (Box::new(Query::new("mock".into())), "end".into()),
                (Box::new(Skip::default()), "start".into()),
            ),
            Rule::new(
                "start".into(),
                Box::new(Any::default()),
                (Box::new(Query::new("mock".into())), "end".into()),
                (Box::new(Skip::default()), "start".into()),
            ),
        ])
        .err()
        .unwrap()
        {
            TableError::MultipleDef(_) => {}
            e => panic!("Not the right error type: {}", e),
        }
    }

    #[tokio::test]
    async fn success_domain_table() {
        Table::new(vec![Rule::new(
            "start".into(),
            Box::new(
                Domain::new(vec!["../data/china.txt".to_string()])
                    .await
                    .unwrap(),
            ),
            (Box::new(Query::new("mock".into())), "end".into()),
            (Box::new(Query::new("another_mock".into())), "end".into()),
        )])
        .ok()
        .unwrap();
    }
}