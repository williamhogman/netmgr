use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize)]
pub struct Zone {
    pub domain: String,
    private_prefix: String,
    networks: Vec<Network>,
}

pub enum RecordTypeFilter {
    Public,
    Private,
    Both,
}

impl RecordTypeFilter {
    fn public(&self) -> bool {
        match self {
            RecordTypeFilter::Public => true,
            RecordTypeFilter::Private => false,
            RecordTypeFilter::Both => true,
        }
    }
    fn private(&self) -> bool {
        match self {
            RecordTypeFilter::Public => false,
            RecordTypeFilter::Private => true,
            RecordTypeFilter::Both => true,
        }
    }
}

impl Zone {
    pub fn read<P: AsRef<Path>>(path: P) -> Result<Zone> {
        let p = File::open(path)?;
        let zone: Zone = serde_yaml::from_reader(p)?;
        Ok(zone)
    }
    pub fn all_records(&self) -> Vec<Record> {
        self.records(RecordTypeFilter::Both)
    }
    fn records(&self, filter: RecordTypeFilter) -> Vec<Record> {
        let mut records = Vec::new();
        if filter.public() {
            for network in &self.networks {
                records.extend(network.public_records(&self.domain));
            }
        }
        if filter.private() {
            for network in &self.networks {
                records.extend(network.private_records(&self.private_prefix, &self.domain));
            }
        }
        records
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Network {
    name: String,
    root: String,
    servers: Vec<Server>,
}

impl Network {
    fn public_records(&self, domain: &str) -> Vec<Record> {
        let mut recs: Vec<Record> = self
            .servers
            .iter()
            .flat_map(|s| s.public_records(&self.name, &self.root, &domain))
            .collect();
        recs.push(Record::Cname(
            format!("{}.{}", self.name.clone(), domain),
            format!("{}.{}.{}", self.root, self.name, domain),
        ));
        recs
    }
    fn private_records(&self, private_prefix: &str, domain: &str) -> Vec<Record> {
        let mut recs: Vec<Record> = self
            .servers
            .iter()
            .flat_map(|s| s.private_records(&format!("{}.{}", &self.name, private_prefix), &domain))
            .collect();
        recs.push(Record::Cname(
            format!("{}.{}.{}", self.name, private_prefix, domain),
            format!("{}.{}.{}.{}", self.root, self.name, private_prefix, domain),
        ));
        recs
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Server {
    name: String,
    private_ip: String,
    #[serde(default)]
    alias: Vec<String>,
}

impl Server {
    fn public_records(&self, suffix: &str, root: &str, domain: &str) -> Vec<Record> {
        let mut v = Vec::new();
        let root_full = format!("{}.{}.{}", root, suffix, domain);
        if self.name != root {
            v.push(Record::Cname(
                format!("{}.{}.{}", &self.name, suffix, domain),
                root_full.to_string(),
            ));
        }
        v.extend(self.alias.iter().map(|a| {
            Record::Cname(
                format!("{}.{}.{}", a, suffix, domain),
                format!("{}.{}.{}", root, suffix, domain),
            )
        }));
        v
    }
    fn private_records(&self, suffix: &str, domain: &str) -> Vec<Record> {
        let mut v = Vec::new();
        v.push(Record::A(
            format!("{}.{}.{}", self.name, suffix, domain),
            self.private_ip.clone(),
        ));
        v.extend(self.alias.iter().map(|a| {
            Record::Cname(
                format!("{}.{}.{}", a, suffix, domain),
                format!("{}.{}.{}", self.name, suffix, domain),
            )
        }));
        v
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Hash)]
pub enum Record {
    A(String, String),
    Cname(String, String),
}

impl Record {
    pub fn name(&self) -> String {
        match self {
            Record::A(name, _) => name.clone(),
            Record::Cname(name, _) => name.clone(),
        }
    }
    pub fn value(&self) -> String {
        match self {
            Record::A(_, value) => value.clone(),
            Record::Cname(_, value) => value.clone(),
        }
    }
}
