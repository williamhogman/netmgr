mod model;
use anyhow::{anyhow, Result};
use cloudflare::endpoints::{dns, zone};
use cloudflare::framework::{
    apiclient::ApiClient,
    auth::Credentials,
    response::{ApiFailure, ApiResponse, ApiResult},
    Environment, HttpApiClient, HttpApiClientConfig, OrderDirection,
};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Debug)]
struct Config {
    cloudflare_token: String,
}

fn cf_record_to_record(cf: &dns::DnsRecord) -> Option<model::Record> {
    let name = cf.name.to_string();
    match &cf.content {
        dns::DnsContent::A { content } => Some(model::Record::A(name, content.to_string())),
        dns::DnsContent::CNAME { content } => Some(model::Record::Cname(name, content.to_string())),
        _ => None,
    }
}

#[derive(Debug)]
struct Diff {
    superflous: Vec<model::Record>,
    pub missing: Vec<model::Record>,
    pub changed: Vec<(model::Record, model::Record)>,
}

impl Diff {
    pub fn new(a: Vec<model::Record>, b: Vec<model::Record>) -> Self {
        let mut superflous = Vec::new();
        let mut missing = Vec::new();
        let mut changed = Vec::new();

        let a: HashMap<String, model::Record> = a.into_iter().map(|r| (r.name(), r)).collect();
        let b: HashMap<String, model::Record> = b.into_iter().map(|r| (r.name(), r)).collect();

        for (name, record) in a.iter() {
            if !b.contains_key(name) {
                superflous.push(record.clone());
            } else if record != b.get(name).unwrap() {
                changed.push((b.get(name).unwrap().clone(), record.clone()));
            }
        }
        for (name, record) in b.iter() {
            if !a.contains_key(name) {
                missing.push(record.clone());
            }
        }
        Diff {
            superflous,
            missing,
            changed,
        }
    }
}

impl From<model::Record> for dns::DnsContent {
    fn from(r: model::Record) -> Self {
        match r {
            model::Record::A(name, ip) => dns::DnsContent::A {
                content: ip.parse().unwrap(),
            },
            model::Record::Cname(name, cname) => dns::DnsContent::CNAME { content: cname },
        }
    }
}
fn main() -> Result<()> {
    let env: &'static Config = Box::leak(Box::new(envy::from_env::<Config>()?));

    let zone = model::Zone::read("./config.yaml")?;
    let recs = zone.all_records();

    let api_client = get_api_client(env)?;
    let zone_identifier = find_zone_id(&api_client, zone)?;
    let (record_ids, cf_recs) = get_current_records(&api_client, &zone_identifier)?;

    let d = Diff::new(cf_recs, recs);

    for (a, _b) in d.changed {
        let resp = update_record(&zone_identifier, &record_ids, a, &api_client)?;
        println!("{:?}", resp);
    }
    for record in d.missing {
        let resp = create_record(&zone_identifier, record, &api_client)?;
        println!("{:?}", resp);
    }
    Ok(())
}

fn create_record(
    zone_identifier: &String,
    record: model::Record,
    api_client: &HttpApiClient,
) -> Result<cloudflare::framework::response::ApiSuccess<dns::DnsRecord>, anyhow::Error> {
    let req = dns::CreateDnsRecord {
        zone_identifier: zone_identifier,
        params: dns::CreateDnsRecordParams {
            ttl: None,
            priority: None,
            proxied: Some(false),
            name: &record.name(),
            content: record.into(),
        },
    };
    let resp = api_client.request(&req)?;
    Ok(resp)
}

fn get_api_client(env: &Config) -> Result<HttpApiClient, anyhow::Error> {
    let credentials = Credentials::UserAuthToken {
        token: env.cloudflare_token.to_string(),
    };
    let api_client = HttpApiClient::new(
        credentials,
        HttpApiClientConfig::default(),
        Environment::Production,
    )?;
    Ok(api_client)
}

fn get_current_records(
    api_client: &HttpApiClient,
    zone_identifier: &str,
) -> Result<(HashMap<String, String>, Vec<model::Record>), anyhow::Error> {
    let list_dns_records = &dns::ListDnsRecords {
        zone_identifier: &zone_identifier,
        params: Default::default(),
    };
    let dns_records = api_client.request(list_dns_records)?.result;
    let record_ids: HashMap<String, String> = dns_records
        .iter()
        .map(|r| (r.name.to_string(), r.id.to_string()))
        .collect();
    let cf_recs: Vec<model::Record> = dns_records.iter().flat_map(cf_record_to_record).collect();
    Ok((record_ids, cf_recs))
}

fn find_zone_id(api_client: &HttpApiClient, zone: model::Zone) -> Result<String> {
    let z = &zone::ListZones {
        params: Default::default(),
    };
    let zones = api_client.request(z)?.result;
    let cf_zone = zones
        .into_iter()
        .find(|z| z.name == zone.domain)
        .ok_or(anyhow!("Unable to find the zone in your account"))?;
    Ok(cf_zone.id)
}

fn update_record(
    zone_identifier: &str,
    record_ids: &HashMap<String, String>,
    new_value: model::Record,
    api_client: &HttpApiClient,
) -> Result<dns::DnsRecord> {
    let update_dns_record = &dns::UpdateDnsRecord {
        zone_identifier: zone_identifier,
        identifier: record_ids
            .get(&new_value.name())
            .ok_or(anyhow!("Unable to find record id"))?,
        params: dns::UpdateDnsRecordParams {
            proxied: Some(false),
            ttl: None,
            name: &new_value.name(),
            content: match new_value {
                model::Record::A(..) => dns::DnsContent::A {
                    content: new_value.value().parse()?,
                },
                model::Record::Cname(..) => dns::DnsContent::CNAME {
                    content: new_value.value(),
                },
            },
        },
    };
    let resp = api_client.request(update_dns_record)?.result;
    Ok(resp)
}
