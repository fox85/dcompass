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

use super::{super::super::parsed::ParsedMatcher, Matcher, Result};
use dmatcher::domain::Domain as DomainAlg;
use tokio::{fs::File, prelude::*};
use trust_dns_proto::{op::query::Query, rr::resource::Record};

pub struct Domain(DomainAlg);

impl Domain {
    pub async fn new(spec: ParsedMatcher) -> Result<Self> {
        Ok(match spec {
            ParsedMatcher::Domain(p) => {
                let mut matcher = DomainAlg::new();
                for r in p {
                    let mut file = File::open(r).await?;
                    let mut data = String::new();
                    file.read_to_string(&mut data).await?;
                    matcher.insert_multi(&data);
                }
                Self(matcher)
            }
            _ => unreachable!(),
        })
    }
}

impl Matcher for Domain {
    fn matches(&self, queries: &[Query], _: &[Record]) -> bool {
        self.0.matches(&queries[0].name().to_utf8())
    }
}
