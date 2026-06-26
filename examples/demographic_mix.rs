//! Aggregate the demographic mix of a list of names.
//!
//! Run with an API key in the environment:
//!
//! ```sh
//! DEMOGRAFIX_API_KEY=YOUR_API_KEY cargo run --example demographic_mix
//! ```
//!
//! The script batches one list of names across all three services and prints a
//! gender split, an age distribution, and a nationality mix. It never labels an
//! individual.

use demografix::{Demografix, Error};
use std::collections::HashMap;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Error> {
    let api_key = std::env::var("DEMOGRAFIX_API_KEY")
        .expect("set DEMOGRAFIX_API_KEY to your Demografix API key");
    let client = Demografix::new(&api_key);

    let names = ["michael", "matthew", "jane", "lois", "nguyen"];

    // Gender split across the list.
    let genders = client.genderize_batch(&names, None).await?;
    let mut gender_split: HashMap<String, usize> = HashMap::new();
    for prediction in &genders.results {
        let label = prediction
            .gender
            .clone()
            .unwrap_or_else(|| "unknown".into());
        *gender_split.entry(label).or_default() += 1;
    }
    println!("Gender split: {gender_split:?}");

    // Age distribution across the list.
    let ages = client.agify_batch(&names, None).await?;
    let known_ages: Vec<i64> = ages.results.iter().filter_map(|p| p.age).collect();
    if known_ages.is_empty() {
        println!("Age distribution: no matches");
    } else {
        let mean = known_ages.iter().sum::<i64>() as f64 / known_ages.len() as f64;
        println!(
            "Age distribution: {} predicted, mean {:.1}",
            known_ages.len(),
            mean
        );
    }

    // Nationality mix across the list: count the top country per name.
    let nationalities = client.nationalize_batch(&names).await?;
    let mut country_mix: HashMap<String, usize> = HashMap::new();
    for prediction in &nationalities.results {
        if let Some(top) = prediction.country.first() {
            *country_mix.entry(top.country_id.clone()).or_default() += 1;
        }
    }
    println!("Nationality mix (top country per name): {country_mix:?}");

    println!("Quota remaining: {}", nationalities.quota.remaining);
    Ok(())
}
