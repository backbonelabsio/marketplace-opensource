use std::str::FromStr;
use cosmwasm_std::{
    entry_point,
    from_binary,
    to_json_binary,
    Binary,
    Deps,
    DepsMut,
    Env,
    MessageInfo,
    Response,
    StdError,
    StdResult,
    Uint128,
    Decimal,
    CosmosMsg,
    Event,
    WasmMsg,
};
use cw721::Cw721ReceiveMsg;
use cw20::{ Balance, Cw20Coin, Cw20CoinVerified, Cw20ExecuteMsg, Cw20ReceiveMsg };
use marketplace::auction::{
    Cw721HookMsg,
    ExecuteMsg,
    InstantiateMsg,
    MigrateMsg,
    QueryMsg,
    ReceiveMsg,
    DenomMsg,
    Cw20PlaceBidMsg,
};

use crate::auction::{
    admin_cancel_auction,
    admin_change_config,
    admin_pause,
    admin_resume,
    cancel_auction,
    create_auction,
    place_bid,
    set_royalty_admin,
    set_royalty_fee,
    settle_auction,
    settle_hook,
};
use crate::error::ContractError;
use crate::querier::{
    construct_action_response,
    query_all_royalty,
    query_auction,
    query_auction_by_amount,
    query_auction_by_bidder,
    query_auction_by_end_time,
    query_auction_by_nft,
    query_auction_by_seller,
    query_bid_history_by_auction_id,
    query_bid_number,
    query_calculate_price,
    query_config,
    query_nft_auction_map,
    query_not_started_auctions,
    query_royalty_admin,
    query_royalty_fee,
    query_state,
};
use crate::state::{ Config, State, CONFIG, STATE };


#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg
) -> Result<Response, ContractError> {
    let config = Config {
        owner: info.sender,
        protocol_fee: msg.protocol_fee,
        min_reserve_price: msg.min_reserve_price,
        max_royalty_fee: msg.max_royalty_fee,
        duration: msg.duration,
        extension_duration: msg.extension_duration,
        min_increment: msg.min_increment,
        accepted_denom: msg.accepted_denom,
        collector_address: deps.api.addr_validate(&msg.collector_address)?,
    };
    if msg.max_royalty_fee + msg.protocol_fee >= Decimal::from_str("1").unwrap() {
        return Err(ContractError::InvalidRoyaltyFee {});
    }
    CONFIG.save(deps.storage, &config)?;

    let state = State {
        next_auction_id: Uint128::from(1u128),
        is_freeze: false,
    };

    STATE.save(deps.storage, &state)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ReceiveNft(msg) => receive_nft(deps, env, info, msg),
        ExecuteMsg::Receive(msg) => execute_receive(deps, env, info, msg),
        ExecuteMsg::PlaceBid { auction_id, cw_balance } => {
            match cw_balance {
                Some(cw20::Balance::Native(_)) => todo!(),
                Some(Balance::Cw20(cw20_coin)) => {
                    // cw_balance is Some(Cw20), extract cw20_coin and call place_bid
                    place_bid(deps, env, info, auction_id, Some(cw20::Balance::Cw20(cw20_coin)))
                },
                None => {
                    // cw_balance is None, call place_bid without a balance
                    place_bid(deps, env, info, auction_id, None)
                }
            }
        },
        ExecuteMsg::Settle { auction_id } => settle_auction(deps, env, auction_id),
        ExecuteMsg::CancelAuction { auction_id } => cancel_auction(deps, env, info, auction_id),
        ExecuteMsg::AdminCancelAuction { auction_id } => {
            admin_cancel_auction(deps, env, info, auction_id)
        }
        ExecuteMsg::AdminPause {} => admin_pause(deps, env, info),
        ExecuteMsg::AdminResume {} => admin_resume(deps, env, info),
        ExecuteMsg::AdminChangeConfig {
            protocol_fee,
            min_increment,
            min_reserve_price,
            max_royalty_fee,
            duration,
            extension_duration,
            accepted_denom,
            collector_address,
        } =>
            admin_change_config(
                deps,
                env,
                info,
                protocol_fee,
                min_increment,
                min_reserve_price,
                max_royalty_fee,
                duration,
                extension_duration,
                accepted_denom,
                collector_address
            ),
        ExecuteMsg::SetRoyaltyFee { contract_addr, royalty_fee, creator } =>
            set_royalty_fee(deps, env, info, contract_addr, creator, royalty_fee),
        ExecuteMsg::SetRoyaltyAdmin { address, enable } => {
            set_royalty_admin(deps, env, info, address, enable)
        }
        ExecuteMsg::SettleHook { nft_contract, token_id, owner } =>
            settle_hook(deps, env, info, nft_contract, token_id, owner),
    }
}

pub fn receive_nft(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw721_msg: Cw721ReceiveMsg
) -> Result<Response, ContractError> {
    match from_binary(&cw721_msg.msg) {
        Ok(Cw721HookMsg::CreateAuction { denom, reserve_price, is_instant_sale }) => {
            // need to check that this contract is owner of nft to prevent malicious contract call this function directly

            let seller = deps.api.addr_validate(&cw721_msg.sender)?;
            let nft_contract = info.sender;
            let token_id = cw721_msg.token_id.clone();
            create_auction(
                deps,
                env,
                nft_contract,
                token_id,
                seller,
                denom,
                reserve_price,
                is_instant_sale
            )
        }
        Err(err) => Err(ContractError::Std(StdError::generic_err(err.to_string()))),
    }
}

pub fn execute_receive(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    wrapper: Cw20ReceiveMsg
) -> Result<Response, ContractError> {
    let msg: ReceiveMsg = from_binary(&wrapper.msg)?;

    let cw_balance = Balance::Cw20(Cw20CoinVerified {
        address: info.sender.clone(), //info.sender is the CW20 Token contract
        amount: wrapper.amount,
    });

    //Make new info with the sender set to who actually sent the cw20 tokens
    let api = deps.api;
    let wrapper_sender = &api.addr_validate(&wrapper.sender)?;
    let new_info = MessageInfo {
        sender: wrapper_sender.clone(),
        funds: vec![],
    };

    match msg {
        ReceiveMsg::PlaceBidCw20(msg) => {
            let auction_id = msg.auction_id;
            let result = place_bid(deps, env, new_info, auction_id, Some(cw_balance))?;
            // Handle the result appropriately
            Ok(result)}
        _ => {
            return Err(ContractError::TracePoint {
                location: "Unknown Message".to_string(),
            });
        }
    }
}



#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
