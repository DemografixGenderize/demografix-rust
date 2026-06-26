# demografix (Rust)

Run demographic analysis over names — predicted gender, age, and nationality — from one async Rust client. The crate covers [genderize.io](https://genderize.io), [agify.io](https://agify.io), and [nationalize.io](https://nationalize.io).

## Install

Add the crate to a project:

```sh
cargo add demografix
```

The client is async and depends on a Tokio runtime. A synchronous surface, `BlockingDemografix`, is available behind the `blocking` feature, which is off by default. See the blocking section below.

## Quickstart

Construct a client, batch over a list of names, read the predictions, and read the remaining quota.

```rust
use demografix::Demografix;

#[tokio::main]
async fn main() -> Result<(), demografix::Error> {
    // api_key is required.
    let client = Demografix::new("YOUR_API_KEY");

    let names = ["michael", "matthew", "jane"];
    let ages = client.agify_batch(&names, None).await?;

    // Aggregate the list into an age distribution.
    let predicted: Vec<i64> = ages.results.iter().filter_map(|p| p.age).collect();
    let mean = predicted.iter().sum::<i64>() as f64 / predicted.len() as f64;
    println!("mean predicted age across {} names: {:.1}", predicted.len(), mean);

    // Remaining quota for the response.
    println!("remaining: {}", ages.quota.remaining);
    Ok(())
}
```

## Usage

Each service has a single-name method and a batch method. A batch accepts at most 10 names. A single-name result derefs to its prediction, so the prediction fields read directly off the result (`result.gender`) and `result.quota` reads the quota. The prediction is also reachable explicitly as `result.prediction`. Batch methods return a `Batch` with `results` and one `quota`.

### Genderize

```rust
// Single name. Prediction fields read straight off the result through Deref.
let result = client.genderize("peter", None).await?;
result.gender;      // Some("male")
result.probability; // 1.0
result.quota.remaining;

// A list, reduced to a gender split.
let names = ["michael", "lois", "jane", "peter"];
let batch = client.genderize_batch(&names, None).await?;
let mut split = std::collections::HashMap::new();
for p in &batch.results {
    let label = p.gender.clone().unwrap_or_else(|| "unknown".into());
    *split.entry(label).or_insert(0) += 1;
}
// split is the aggregate: how the list breaks down by gender.
```

### Agify

```rust
// Single name.
let result = client.agify("michael", None).await?;
result.age; // Some(57)

// A list, reduced to an age distribution.
let batch = client.agify_batch(&["michael", "matthew", "jane"], None).await?;
let ages: Vec<i64> = batch.results.iter().filter_map(|p| p.age).collect();
```

### Nationalize

```rust
// Single name.
let result = client.nationalize("nguyen").await?;
result.country[0].country_id; // "VN"

// A list, reduced to a nationality mix.
let batch = client.nationalize_batch(&["nguyen", "smith", "garcia"]).await?;
let mut mix = std::collections::HashMap::new();
for p in &batch.results {
    if let Some(top) = p.country.first() {
        *mix.entry(top.country_id.clone()).or_insert(0) += 1;
    }
}
// mix is the aggregate: the top country per name across the list.
```

## country_id

`genderize` and `agify` accept an optional `country_id` (ISO 3166-1 alpha-2) to scope the prediction to a country. Pass it as the second argument. The API echoes it back uppercase in `country_id`. `nationalize` takes no `country_id`.

```rust
let result = client.genderize("kim", Some("US")).await?;
result.country_id; // Some("US")

let batch = client.agify_batch(&["kim", "andrea"], Some("DK")).await?;
```

## Quota

Every result and every error carries a `Quota` read from the rate-limit response headers. Quota is read off a returned value or a raised error. It is never cached on the client.

| Field | Meaning |
|---|---|
| `limit` | names allowed in the current window |
| `remaining` | names left in the current window |
| `reset` | seconds until the window resets |

```rust
let result = client.genderize("peter", None).await?;
result.quota.limit;     // 25000
result.quota.remaining; // 24987
result.quota.reset;     // 1314000
```

## Errors

Every method returns `Result<T, Error>`. `Error` is a single enum whose variants map to the cross-language error hierarchy by HTTP status:

| Variant | Cause |
|---|---|
| `Error::Auth` | 401, invalid or missing API key |
| `Error::Subscription` | 402, subscription problem |
| `Error::Validation` | 422, or a batch over 10 names rejected before any HTTP call |
| `Error::RateLimit` | 429, rate limit reached; quota is always populated |
| `Error::Api` | any other non-2xx status |
| `Error::Transport` | network failure, timeout, or a non-JSON body |

Read the status, message, and quota through the accessor methods `status()`, `message()`, and `quota()`. A batch over 10 names raises `Error::Validation` client-side, before any request goes out.

On a `RateLimitError`, `quota.reset` reports how many seconds remain before the window resets. Use it to back off:

```rust
use demografix::{Demografix, Error};

async fn genderize_with_backoff(
    client: &Demografix,
    names: &[&str],
) -> Result<Vec<Option<String>>, Error> {
    loop {
        match client.genderize_batch(names, None).await {
            Ok(batch) => {
                return Ok(batch.results.iter().map(|p| p.gender.clone()).collect());
            }
            Err(Error::RateLimit { quota, .. }) => {
                let wait = std::time::Duration::from_secs(quota.reset.max(0) as u64);
                tokio::time::sleep(wait).await;
            }
            Err(other) => return Err(other),
        }
    }
}
```

## Methods reference

| Method | Returns | country_id |
|---|---|---|
| `genderize(name, country_id)` | `GenderizeResult` | yes |
| `genderize_batch(names, country_id)` | `Batch<GenderizePrediction>` | yes |
| `agify(name, country_id)` | `AgifyResult` | yes |
| `agify_batch(names, country_id)` | `Batch<AgifyPrediction>` | yes |
| `nationalize(name)` | `NationalizeResult` | no |
| `nationalize_batch(names)` | `Batch<NationalizePrediction>` | no |

Construct the client with `Demografix::new(api_key)` for the default 10-second timeout, or `Demografix::with_timeout(api_key, duration)` to set your own. The API key is required. An empty or blank key makes every request fail with `Error::Validation` before any HTTP call.

## Blocking

A synchronous client is available behind the `blocking` feature, which is off by default. Enable it for code that does not run on an async runtime:

```toml
[dependencies]
demografix = { version = "0.1.0", features = ["blocking"] }
```

`BlockingDemografix` mirrors the async surface method for method, with the same models, errors, and quota. The methods return their results directly, without `.await`.

```rust
use demografix::BlockingDemografix;

fn main() -> Result<(), demografix::Error> {
    let client = BlockingDemografix::new("YOUR_API_KEY");

    let names = ["michael", "matthew", "jane"];
    let batch = client.agify_batch(&names, None)?;

    // Aggregate the list into an age distribution.
    let ages: Vec<i64> = batch.results.iter().filter_map(|p| p.age).collect();
    let mean = ages.iter().sum::<i64>() as f64 / ages.len() as f64;
    println!("mean predicted age across {} names: {:.1}", ages.len(), mean);
    Ok(())
}
```

## API keys

An API key is required. Creating one is free and includes 2,500 requests per month. Generate a key in your dashboard at [genderize.io](https://genderize.io), [agify.io](https://agify.io), or [nationalize.io](https://nationalize.io). One key works across all three services.

Full API reference: <https://genderize.io/documentation/api>

## License

MIT. See [LICENSE](LICENSE).
