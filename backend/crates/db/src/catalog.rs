//! What a customer can put in a test order (demo only). Names, prices and
//! categories are ours, never the client's; categories are the seed's six so
//! the policy's scoped windows apply. Two final-sale items let that rule be
//! tried.

use serde::Serialize;

use crate::seed::{
    ACCESSORIES, DAY_PASSES, MEMBERSHIPS, OFFICE_DEPOSITS, ROOM_BOOKINGS, SUBSCRIPTIONS,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Workspace,
    Services,
    Products,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Workspace, Group::Services, Group::Products];

    pub fn label(self) -> &'static str {
        match self {
            Group::Workspace => "Workspace & bookings",
            Group::Services => "Add-ons & services",
            Group::Products => "Accessories & products",
        }
    }
}

/// How the item is used, which suggests a delivery status for the order.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Booking,
    Plan,
    Deposit,
    Service,
    Product,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CatalogItem {
    pub id: &'static str,
    pub group: Group,
    pub name: &'static str,
    pub kind: Kind,
    pub unit_cents: i64,
    /// e.g. "per hour", "1 month", "each".
    pub unit: &'static str,
    /// Priced per hour; the quantity is hours.
    pub hourly: bool,
    pub category: &'static str,
    pub final_sale: bool,
}

const fn it(
    id: &'static str,
    group: Group,
    name: &'static str,
    kind: Kind,
    dollars: i64,
    unit: &'static str,
    category: &'static str,
) -> CatalogItem {
    CatalogItem {
        id,
        group,
        name,
        kind,
        unit_cents: dollars * 100,
        unit,
        hourly: false,
        category,
        final_sale: false,
    }
}

const fn hourly(mut item: CatalogItem) -> CatalogItem {
    item.hourly = true;
    item
}

const fn final_sale(mut item: CatalogItem) -> CatalogItem {
    item.final_sale = true;
    item
}

use Group::{Products, Services, Workspace};
use Kind::{Booking, Deposit, Plan, Product, Service};

pub const CATALOG: &[CatalogItem] = &[
    it(
        "day-pass", Workspace, "Day Pass", Booking, 25, "per pass", DAY_PASSES,
    ),
    it(
        "flex-10",
        Workspace,
        "Flex Day Pass, 10-pack",
        Plan,
        180,
        "10 passes",
        DAY_PASSES,
    ),
    it(
        "hot-desk-week",
        Workspace,
        "Hot Desk, weekly",
        Plan,
        79,
        "1 week",
        MEMBERSHIPS,
    ),
    it(
        "hot-desk-month",
        Workspace,
        "Hot Desk, monthly",
        Plan,
        249,
        "1 month",
        MEMBERSHIPS,
    ),
    final_sale(it(
        "hot-desk-month-sale",
        Workspace,
        "Hot Desk, monthly (discounted)",
        Plan,
        199,
        "1 month",
        MEMBERSHIPS,
    )),
    it(
        "dedicated-desk",
        Workspace,
        "Dedicated Desk, monthly",
        Plan,
        329,
        "1 month",
        MEMBERSHIPS,
    ),
    hourly(it(
        "meeting-room",
        Workspace,
        "Meeting Room",
        Booking,
        24,
        "per hour",
        ROOM_BOOKINGS,
    )),
    hourly(it(
        "podcast-studio",
        Workspace,
        "Podcast Studio",
        Booking,
        45,
        "per hour",
        ROOM_BOOKINGS,
    )),
    it(
        "event-space",
        Workspace,
        "Event Space, half day",
        Booking,
        420,
        "half day",
        ROOM_BOOKINGS,
    ),
    it(
        "private-office",
        Workspace,
        "Private Office, monthly deposit",
        Deposit,
        950,
        "deposit",
        OFFICE_DEPOSITS,
    ),
    it(
        "virtual-office",
        Workspace,
        "Virtual Office, annual",
        Plan,
        588,
        "12 months",
        MEMBERSHIPS,
    ),
    it(
        "locker-1",
        Workspace,
        "Locker Rental, 1 month",
        Plan,
        20,
        "1 month",
        MEMBERSHIPS,
    ),
    it(
        "locker-3",
        Workspace,
        "Locker Rental, 3 months",
        Plan,
        45,
        "3 months",
        MEMBERSHIPS,
    ),
    it(
        "coffee-add-on",
        Services,
        "Coffee add-on",
        Booking,
        8,
        "per booking",
        ROOM_BOOKINGS,
    ),
    it(
        "printing-100",
        Services,
        "Printing Credits, 100 pages",
        Service,
        10,
        "100 pages",
        SUBSCRIPTIONS,
    ),
    it(
        "printing-500",
        Services,
        "Printing Credits, 500 pages",
        Service,
        30,
        "500 pages",
        SUBSCRIPTIONS,
    ),
    it(
        "mail-handling",
        Services,
        "Mail Handling, quarterly",
        Plan,
        60,
        "3 months",
        SUBSCRIPTIONS,
    ),
    it(
        "coffee-subscription",
        Services,
        "Coffee Subscription",
        Plan,
        28,
        "per month",
        SUBSCRIPTIONS,
    ),
    it(
        "desk-lamp",
        Products,
        "Worknoon Desk Lamp",
        Product,
        62,
        "each",
        ACCESSORIES,
    ),
    it(
        "usb-c-hub",
        Products,
        "USB-C Hub",
        Product,
        24,
        "each",
        ACCESSORIES,
    ),
    it(
        "laptop-stand",
        Products,
        "Laptop Stand",
        Product,
        48,
        "each",
        ACCESSORIES,
    ),
    it(
        "keyboard",
        Products,
        "Wireless Keyboard",
        Product,
        69,
        "each",
        ACCESSORIES,
    ),
    it(
        "headset",
        Products,
        "Noise-cancelling Headset",
        Product,
        129,
        "each",
        ACCESSORIES,
    ),
    it(
        "mug",
        Products,
        "Worknoon Mug",
        Product,
        14,
        "each",
        ACCESSORIES,
    ),
    it(
        "notebook",
        Products,
        "Worknoon Notebook, A5",
        Product,
        12,
        "each",
        ACCESSORIES,
    ),
    final_sale(it(
        "hoodie-clearance",
        Products,
        "Worknoon Hoodie (clearance)",
        Product,
        35,
        "each",
        ACCESSORIES,
    )),
];

pub fn find(id: &str) -> Option<&'static CatalogItem> {
    CATALOG.iter().find(|i| i.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_every_group_has_items() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|i| i.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());
        for g in Group::ALL {
            assert!(CATALOG.iter().any(|i| i.group == g), "{g:?}");
        }
        assert_eq!(CATALOG.iter().filter(|i| i.final_sale).count(), 2);
    }
}
