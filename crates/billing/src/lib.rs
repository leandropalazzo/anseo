//! Story 24.1 — Stripe subscription sync and entitlement mapping.
//!
//! This crate is runtime-gated by caller configuration: API boot can compile
//! without Stripe secrets, and live Stripe calls only happen after a caller
//! constructs [`StripeClient`] with `STRIPE_SECRET_KEY`.

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("stripe request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("stripe webhook verification failed: {0}")]
    Webhook(#[from] stripe_webhook::WebhookError),
    #[error("invalid stripe webhook payload: {0}")]
    InvalidPayload(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Plan {
    Free,
    Pro,
    Enterprise,
}

impl Plan {
    pub fn as_str(self) -> &'static str {
        match self {
            Plan::Free => "free",
            Plan::Pro => "pro",
            Plan::Enterprise => "enterprise",
        }
    }
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Plan {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "free" => Ok(Plan::Free),
            "pro" => Ok(Plan::Pro),
            "enterprise" => Ok(Plan::Enterprise),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Active,
    Trialing,
    PastDue,
    Canceled,
    Unpaid,
    Incomplete,
    IncompleteExpired,
    Paused,
    Unknown(String),
}

impl SubscriptionStatus {
    fn from_stripe(status: &str) -> Self {
        match status {
            "active" => Self::Active,
            "trialing" => Self::Trialing,
            "past_due" => Self::PastDue,
            "canceled" => Self::Canceled,
            "unpaid" => Self::Unpaid,
            "incomplete" => Self::Incomplete,
            "incomplete_expired" => Self::IncompleteExpired,
            "paused" => Self::Paused,
            other => Self::Unknown(other.to_string()),
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self, Self::Active | Self::Trialing | Self::PastDue)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrgEntitlement {
    pub plan: Plan,
    pub seat_count: u32,
    pub status: SubscriptionStatus,
}

pub struct StripeClient {
    secret_key: String,
    http: reqwest::Client,
    _stripe: stripe::Client,
}

impl StripeClient {
    pub fn new(secret_key: &str) -> Self {
        Self {
            secret_key: secret_key.to_string(),
            http: reqwest::Client::new(),
            _stripe: stripe::Client::new(secret_key),
        }
    }

    pub async fn sync_subscription(&self, customer_id: &str) -> Result<OrgEntitlement, Error> {
        let response = self
            .http
            .get("https://api.stripe.com/v1/subscriptions")
            .bearer_auth(&self.secret_key)
            .query(&[
                ("customer", customer_id),
                ("status", "all"),
                ("limit", "1"),
                ("expand[]", "data.items.data.price"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json::<SubscriptionList>()
            .await?;

        let Some(subscription) = response.data.into_iter().next() else {
            return Ok(OrgEntitlement {
                plan: Plan::Free,
                seat_count: 0,
                status: SubscriptionStatus::Canceled,
            });
        };

        Ok(subscription_to_entitlement(&subscription, false))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookSubscriptionKind {
    Updated,
    Deleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookSubscriptionUpdate {
    pub kind: WebhookSubscriptionKind,
    pub subscription_id: String,
    pub customer_id: Option<String>,
    pub entitlement: OrgEntitlement,
}

pub fn parse_subscription_webhook(
    payload: &str,
    signature: &str,
    secret: &str,
) -> Result<Option<WebhookSubscriptionUpdate>, Error> {
    let _event = stripe_webhook::Webhook::construct_event(payload, signature, secret)?;
    let value: serde_json::Value = serde_json::from_str(payload)?;
    let event_type = value
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();

    if !matches!(
        event_type,
        "customer.subscription.updated" | "customer.subscription.deleted"
    ) {
        return Ok(None);
    }

    let Some(object) = value.get("data").and_then(|d| d.get("object")) else {
        return Ok(None);
    };
    let subscription: Subscription = serde_json::from_value(object.clone())?;
    let deleted = event_type == "customer.subscription.deleted";
    let entitlement = subscription_to_entitlement(&subscription, deleted);

    Ok(Some(WebhookSubscriptionUpdate {
        kind: if deleted {
            WebhookSubscriptionKind::Deleted
        } else {
            WebhookSubscriptionKind::Updated
        },
        customer_id: subscription.customer_id(),
        subscription_id: subscription.id,
        entitlement,
    }))
}

fn subscription_to_entitlement(subscription: &Subscription, deleted: bool) -> OrgEntitlement {
    if deleted {
        return OrgEntitlement {
            plan: Plan::Free,
            seat_count: 0,
            status: SubscriptionStatus::Canceled,
        };
    }

    let status = SubscriptionStatus::from_stripe(&subscription.status);
    let plan = infer_plan(subscription, &status);
    let seat_count = infer_seat_count(subscription, plan);

    OrgEntitlement {
        plan,
        seat_count,
        status,
    }
}

fn infer_plan(subscription: &Subscription, status: &SubscriptionStatus) -> Plan {
    if !status.is_live() {
        return Plan::Free;
    }

    metadata_plan(&subscription.metadata)
        .or_else(|| {
            subscription.items.data.iter().find_map(|item| {
                metadata_plan(&item.metadata)
                    .or_else(|| item.price.as_ref().and_then(Price::plan_hint))
            })
        })
        .unwrap_or(Plan::Pro)
}

fn infer_seat_count(subscription: &Subscription, plan: Plan) -> u32 {
    if plan == Plan::Free {
        return 0;
    }

    subscription
        .metadata
        .as_ref()
        .and_then(|m| parse_u32(m.get("seat_count")).or_else(|| parse_u32(m.get("seats"))))
        .or_else(|| {
            subscription
                .items
                .data
                .iter()
                .filter_map(|item| item.quantity)
                .find(|qty| *qty > 0)
                .and_then(|qty| u32::try_from(qty).ok())
        })
        .unwrap_or(1)
}

fn metadata_plan(metadata: &Option<HashMap<String, String>>) -> Option<Plan> {
    metadata
        .as_ref()
        .and_then(|m| m.get("anseo_plan").or_else(|| m.get("plan")))
        .and_then(|plan| Plan::from_str(plan).ok())
}

fn parse_u32(value: Option<&String>) -> Option<u32> {
    value.and_then(|v| v.parse::<u32>().ok())
}

#[derive(Debug, Deserialize)]
struct SubscriptionList {
    data: Vec<Subscription>,
}

#[derive(Debug, Deserialize)]
struct Subscription {
    id: String,
    status: String,
    customer: Option<StripeRef>,
    #[serde(default)]
    metadata: Option<HashMap<String, String>>,
    #[serde(default)]
    items: SubscriptionItems,
}

impl Subscription {
    fn customer_id(&self) -> Option<String> {
        match &self.customer {
            Some(StripeRef::Id(id)) => Some(id.clone()),
            Some(StripeRef::Object { id }) => Some(id.clone()),
            None => None,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct SubscriptionItems {
    #[serde(default)]
    data: Vec<SubscriptionItem>,
}

#[derive(Debug, Deserialize)]
struct SubscriptionItem {
    #[serde(default)]
    quantity: Option<i64>,
    #[serde(default)]
    metadata: Option<HashMap<String, String>>,
    #[serde(default)]
    price: Option<Price>,
}

#[derive(Debug, Deserialize)]
struct Price {
    #[serde(default)]
    lookup_key: Option<String>,
    #[serde(default)]
    nickname: Option<String>,
    #[serde(default)]
    metadata: Option<HashMap<String, String>>,
}

impl Price {
    fn plan_hint(&self) -> Option<Plan> {
        metadata_plan(&self.metadata).or_else(|| {
            self.lookup_key
                .as_deref()
                .or(self.nickname.as_deref())
                .and_then(|hint| {
                    let hint = hint.to_ascii_lowercase();
                    if hint.contains("enterprise") {
                        Some(Plan::Enterprise)
                    } else if hint.contains("pro") {
                        Some(Plan::Pro)
                    } else if hint.contains("free") {
                        Some(Plan::Free)
                    } else {
                        None
                    }
                })
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StripeRef {
    Id(String),
    Object { id: String },
}

// ---------------------------------------------------------------------------
// Story 24.2 / 25.1 — plan inclusions + overage helpers
// ---------------------------------------------------------------------------

/// Seats, brands, and capabilities included in each plan at no extra cost.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanInclusions {
    pub included_seats: u32,
    pub included_brands: u32,
    /// Whether the plan grants access to org branding (story 25.1).
    pub branding_enabled: bool,
}

/// Returns the plan inclusions for a given plan tier.
pub fn plan_inclusions(plan: Plan) -> PlanInclusions {
    match plan {
        Plan::Free => PlanInclusions {
            included_seats: 1,
            included_brands: 1,
            branding_enabled: false,
        },
        Plan::Pro => PlanInclusions {
            included_seats: 5,
            included_brands: 3,
            branding_enabled: true,
        },
        Plan::Enterprise => PlanInclusions {
            included_seats: 25,
            included_brands: u32::MAX,
            branding_enabled: true,
        },
    }
}

/// Units of overage for a billing cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overage {
    pub seat_overage: u32,
    pub brand_overage: u32,
}

/// Compute overage given a plan and current active counts.
pub fn compute_overage(plan: Plan, active_seats: u32, active_brands: u32) -> Overage {
    if plan == Plan::Free {
        return Overage {
            seat_overage: 0,
            brand_overage: 0,
        };
    }
    let inclusions = plan_inclusions(plan);
    Overage {
        seat_overage: active_seats.saturating_sub(inclusions.included_seats),
        brand_overage: active_brands.saturating_sub(if inclusions.included_brands == u32::MAX {
            active_brands
        } else {
            inclusions.included_brands
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_controls_plan_and_seats() {
        let subscription = Subscription {
            id: "sub_123".into(),
            status: "active".into(),
            customer: Some(StripeRef::Id("cus_123".into())),
            metadata: Some(HashMap::from([
                ("anseo_plan".into(), "enterprise".into()),
                ("seat_count".into(), "42".into()),
            ])),
            items: SubscriptionItems::default(),
        };

        let entitlement = subscription_to_entitlement(&subscription, false);
        assert_eq!(entitlement.plan, Plan::Enterprise);
        assert_eq!(entitlement.seat_count, 42);
        assert_eq!(subscription.customer_id().as_deref(), Some("cus_123"));
    }

    #[test]
    fn deleted_subscription_maps_to_free() {
        let subscription = Subscription {
            id: "sub_123".into(),
            status: "active".into(),
            customer: None,
            metadata: None,
            items: SubscriptionItems::default(),
        };

        let entitlement = subscription_to_entitlement(&subscription, true);
        assert_eq!(entitlement.plan, Plan::Free);
        assert_eq!(entitlement.seat_count, 0);
        assert_eq!(entitlement.status, SubscriptionStatus::Canceled);
    }
}
