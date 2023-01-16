use core::ops::Add;

use crate::{
    error::MarketplaceError, interfaces::icep78::ICEP78, utils::get_current_address, Dict, Listing,
    Status, TokenId,
};
use casper_types::{CLValue, URef};
// Importing Rust types.
use alloc::{
    string::{String, ToString},
    vec,
};
use casper_contract::{
    contract_api::{runtime, storage, system},
    unwrap_or_revert::UnwrapOrRevert,
};
// Importing specific Casper types.
use casper_types::{
    contracts::NamedKeys, runtime_args, CLType, ContractHash, EntryPoint, EntryPointAccess,
    EntryPointType, EntryPoints, Key, Parameter, RuntimeArgs, U256, U512,
};

// Creating constants for the various contract entry points.
const ENTRY_POINT_INIT: &str = "init";
const ENTRY_POINT_ADD_LISTING: &str = "add_listing";
const ENTRY_POINT_CANCEL_LISTING: &str = "cancel_listing";
const ENTRY_POINT_EXECUTE_LISTING: &str = "execute_listing";
const ENTRY_POINT_GET_LISTING: &str = "get_listing";

// Creating constants for the entry point arguments.
// const CONTRACT_NAME_ARG: &str = "contract_name_arg";
const FEE_WALLET_ARG: &str = "fee_wallet";
// const FEE_ARG: &str = "fee";
const COLLECTION_ARG: &str = "collection";
const TOKEN_ID_ARG: &str = "token_id";
const PAY_TOKEN_ARG: &str = "pay_token";
const PRICE_ARG: &str = "price";
const LISTING_ID_ARG: &str = "listing_id";
const PURSE_ARG: &str = "purse";

// Creating constants for values within the contract.
const FEE_WALLET: &str = "fee_wallet";
const LISTINGS_DICT: &str = "listings_dict";
const LISTING_COUNTER: &str = "listing_counter";

#[no_mangle]
pub extern "C" fn init() {
    let fee_wallet_hash = runtime::get_named_arg::<Key>(FEE_WALLET_ARG);
    runtime::put_key(FEE_WALLET, fee_wallet_hash);
    Dict::init(LISTINGS_DICT);
}

#[no_mangle]
pub extern "C" fn add_listing() {
    let marketplace_address = get_current_address(
        runtime::get_call_stack()
            .into_iter()
            .rev()
            .next()
            .unwrap_or_revert(),
    );
    let caller = runtime::get_caller();
    let collection = runtime::get_named_arg::<Key>(COLLECTION_ARG)
        .into_hash()
        .map(ContractHash::new)
        .unwrap();
    let token_id: TokenId = runtime::get_named_arg(TOKEN_ID_ARG);
    let price: U256 = runtime::get_named_arg(PRICE_ARG);
    let pay_token = runtime::get_named_arg::<Key>(PAY_TOKEN_ARG)
        .into_hash()
        .map(ContractHash::new)
        .unwrap();

    if price == U256::from(0u64) {
        runtime::revert(MarketplaceError::ListingPriceIsZero);
    }

    let listing = Listing {
        owner: caller,
        collection,
        token_id,
        pay_token,
        price,
        status: Status::Added,
    };

    let nft = ICEP78::new(collection);
    nft.get_approved(caller.into(), token_id)
        .unwrap_or_revert_with(MarketplaceError::NFTRequireApprove);

    nft.transfer(caller.into(), marketplace_address.into(), token_id);

    let listings = Dict::instance(LISTINGS_DICT);
    let listing_counter_uref = runtime::get_key(LISTING_COUNTER)
        .unwrap()
        .into_uref()
        .unwrap();
    let current_listing_id = storage::read::<u64>(listing_counter_uref).unwrap().unwrap();
    let next_listing_id = current_listing_id.add(1);
    listings.set(&next_listing_id.to_string(), listing);
    storage::add::<u64>(listing_counter_uref, 1u64);
}

#[no_mangle]
pub extern "C" fn cancel_listing() {
    let marketplace_address = get_current_address(
        runtime::get_call_stack()
            .into_iter()
            .rev()
            .next()
            .unwrap_or_revert(),
    );
    let caller = runtime::get_caller();
    let listing_id = runtime::get_named_arg::<u64>(LISTING_ID_ARG);

    let listings = Dict::instance(LISTINGS_DICT);
    let mut listing = listings
        .get::<Listing>(&listing_id.to_string())
        .unwrap_or_revert_with(MarketplaceError::ListingNotFound);

    if listing.owner != caller {
        runtime::revert(MarketplaceError::NoListingOwner);
    }

    if listing.status != Status::Added {
        runtime::revert(MarketplaceError::ListingNotActive);
    }

    listing.status = Status::Cancelled;

    listings.set(&listing_id.to_string(), listing);

    let nft = ICEP78::new(listing.collection);

    nft.transfer(
        marketplace_address.into(),
        listing.owner.into(),
        listing.token_id,
    );
}

#[no_mangle]
pub extern "C" fn execute_listing() {
    let marketplace_address = get_current_address(
        runtime::get_call_stack()
            .into_iter()
            .rev()
            .next()
            .unwrap_or_revert(),
    );

    let caller = runtime::get_caller();
    let listing_id = runtime::get_named_arg::<u64>(LISTING_ID_ARG);
    let caller_purse = runtime::get_named_arg::<URef>(PURSE_ARG);

    let listings = Dict::instance(LISTINGS_DICT);

    let mut listing = listings
        .get::<Listing>(&listing_id.to_string())
        .unwrap_or_revert_with(MarketplaceError::ListingNotFound);

    if listing.status != Status::Added {
        runtime::revert(MarketplaceError::ListingNotActive);
    }

    if listing.owner == caller {
        runtime::revert(MarketplaceError::ListingOwnerCannotBuy);
    }

    listing.status = Status::Executed;
    listings.set(&listing_id.to_string(), listing);

    let nft = ICEP78::new(listing.collection);

    system::transfer_from_purse_to_account(
        caller_purse,
        listing.owner,
        U512::from(listing.price.as_u64()),
        None,
    )
    .unwrap();

    nft.transfer(marketplace_address.into(), caller.into(), listing.token_id);
}

#[no_mangle]
pub extern "C" fn get_listing() {
    let listing_id = runtime::get_named_arg::<u64>(LISTING_ID_ARG);
    let listings = Dict::instance(LISTINGS_DICT);
    let listing = listings
        .get::<Listing>(&listing_id.to_string())
        .unwrap_or_revert_with(MarketplaceError::ListingNotFound);
    runtime::ret(CLValue::from_t(listing.owner).unwrap());
}

//This is the full `call` function as defined within the donation contract.
#[no_mangle]
pub extern "C" fn call() {
    // let contract_name: String = runtime::get_named_arg(CONTRACT_NAME_ARG);
    let fee_wallet: Key = runtime::get_named_arg(FEE_WALLET_ARG);
    // This establishes the `init` entry point for initializing the contract's infrastructure.
    let init_entry_point = EntryPoint::new(
        ENTRY_POINT_INIT,
        vec![Parameter::new(FEE_WALLET_ARG, CLType::Key)],
        CLType::Unit,
        EntryPointAccess::Public,
        EntryPointType::Contract,
    );

    let add_listing_entry_point = EntryPoint::new(
        ENTRY_POINT_ADD_LISTING,
        vec![
            Parameter::new(COLLECTION_ARG, CLType::String),
            Parameter::new(TOKEN_ID_ARG, CLType::U256),
            Parameter::new(PAY_TOKEN_ARG, CLType::String),
            Parameter::new(PRICE_ARG, CLType::U256),
        ],
        CLType::URef,
        EntryPointAccess::Public,
        EntryPointType::Contract,
    );
    let cancel_listing_entry_point = EntryPoint::new(
        ENTRY_POINT_CANCEL_LISTING,
        vec![Parameter::new(LISTING_ID_ARG, CLType::U64)],
        CLType::URef,
        EntryPointAccess::Public,
        EntryPointType::Contract,
    );
    let execute_listing_entry_point = EntryPoint::new(
        ENTRY_POINT_EXECUTE_LISTING,
        vec![
            Parameter::new(LISTING_ID_ARG, CLType::U64),
            Parameter::new(PURSE_ARG, CLType::URef),
        ],
        CLType::URef,
        EntryPointAccess::Public,
        EntryPointType::Contract,
    );
    let get_listing_entry_point = EntryPoint::new(
        ENTRY_POINT_GET_LISTING,
        vec![Parameter::new(LISTING_ID_ARG, CLType::U64)],
        CLType::URef,
        EntryPointAccess::Public,
        EntryPointType::Contract,
    );
    let mut entry_points = EntryPoints::new();
    entry_points.add_entry_point(init_entry_point);
    entry_points.add_entry_point(add_listing_entry_point);
    entry_points.add_entry_point(cancel_listing_entry_point);
    entry_points.add_entry_point(execute_listing_entry_point);
    entry_points.add_entry_point(get_listing_entry_point);

    let listing_count_start = storage::new_uref(0u64);

    let mut named_keys = NamedKeys::new();

    named_keys.insert(String::from(LISTING_COUNTER), listing_count_start.into());

    let (contract_hash, _contract_version) = storage::new_contract(
        entry_points,
        Some(named_keys),
        Some("marketplace_package_hash".to_string()),
        Some("marketplace_access_uref".to_string()),
    );

    runtime::put_key("marketplace_contract_hash", contract_hash.into());
    // Call the init entry point to setup and create the fundraising purse
    // and the ledger to track donations made.
    runtime::call_contract::<()>(
        contract_hash,
        ENTRY_POINT_INIT,
        runtime_args! {
            FEE_WALLET_ARG => fee_wallet,
        },
    )
}
