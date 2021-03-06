#![no_std]

extern crate alloc;

elrond_wasm::imports!();
elrond_wasm::derive_imports!();

const NFT_AMOUNT: u32 = 1;
const ROYALTIES_MAX: u32 = 10_000;

const URI_SLASH: &[u8] = "/".as_bytes();
const HASH_TAG: &[u8] = "#".as_bytes();
const CREATION_TIME_KEY_NAME: &[u8] = "creatime:".as_bytes();
const IMAGE_FILE_EXTENSION: &[u8] = ".png".as_bytes();
const METADATA_FILE_EXTENSION: &[u8] = ".json".as_bytes();

#[elrond_wasm::contract]
pub trait NftManager {
    #[init]
    fn init(&self, payment_token_id: TokenIdentifier, nft_token_price: BigUint, royalties: u32, image_base_uri: ManagedBuffer, metadata_base_uri: ManagedBuffer) -> SCResult<()> {
        require!(royalties <= ROYALTIES_MAX, "royalties cannot exceed 100%");
        require!(
            payment_token_id.is_egld() || payment_token_id.is_valid_esdt_identifier(),
            "invalid token identifier provided"
        );

        self.payment_token_id().set(&payment_token_id);
        self.nft_token_price().set(&nft_token_price);
        self.royalties().set(royalties);
        self.image_base_uri().set(&image_base_uri);
        self.metadata_base_uri().set(&metadata_base_uri);

        // set mint_count to 0 for indexing
        self.mint_count().set(0u32);

        Ok(())
    }

    // endpoints - owner-only

    #[only_owner]
    #[payable("EGLD")]
    #[endpoint(issueNft)]
    fn issue_nft(&self, token_name: ManagedBuffer, token_ticker: ManagedBuffer) -> AsyncCall {
        require!(self.nft_token_id().is_empty(), "Token already issued");

        // save token name
        self.nft_token_name().set(&token_name);

        let payment_amount = self.call_value().egld_value();
        self.send()
            .esdt_system_sc_proxy()
            .issue_non_fungible(
                payment_amount,
                &token_name,
                &token_ticker,
                NonFungibleTokenProperties {
                    can_freeze: false,
                    can_wipe: false,
                    can_pause: false,
                    can_change_owner: true,
                    can_upgrade: false,
                    can_add_special_roles: true,
                },
            )
            .async_call()
            .with_callback(self.callbacks().issue_callback())
    }

    #[only_owner]
    #[endpoint(setLocalRoles)]
    fn set_local_roles(&self) -> AsyncCall {
        self.require_token_issued();

        self.send()
            .esdt_system_sc_proxy()
            .set_special_roles(
                &self.blockchain().get_sc_address(),
                &self.nft_token_id().get(),
                [EsdtLocalRole::NftCreate][..].iter().cloned(),
            )
            .async_call()
    }

    #[only_owner]
    #[endpoint(pauseMinting)]
    fn pause_minting(&self) -> SCResult<()> {
        self.paused().set(true);

        Ok(())
    }

    #[only_owner]
    #[endpoint(startMinting)]
    fn start_minting(&self) -> SCResult<()> {
        require!(!self.nft_token_id().is_empty(), "token not issued");

        self.paused().clear();

        Ok(())
    }

    // return estd of token_id
    // return egld if token_id is not given
    #[only_owner]
    #[endpoint(withdraw)]
    fn withdraw(&self, #[var_args] token_id: OptionalArg<TokenIdentifier>) -> SCResult<()> {
        let payment_token_id = if let OptionalArg::Some(ti) = token_id {
            ti
        }
        else {
            TokenIdentifier::egld()
        };

        let balance = self.blockchain().get_sc_balance(&payment_token_id, 0);
        require!(balance != BigUint::zero(), "not enough balance");

        let caller = self.blockchain().get_caller();
        
        self.send().direct(&caller, &payment_token_id, 0, &balance, &[]);

        Ok(())
    }


    /// endpoint

    #[payable("*")]
    #[endpoint(mint)]
    fn mint(&self, #[payment_token] payment_token: TokenIdentifier, #[payment_amount] payment_amount: BigUint) {
        self.require_token_issued();

        require!(
            payment_token == self.payment_token_id().get(),
            "not given token identifier"
        );
        require!(
            payment_amount >= self.nft_token_price().get(),
            "not enough tokens"
        );

        let nft_nonce = self._mint();
        let nft_token_id = self.nft_token_id().get();
        let caller = self.blockchain().get_caller();
        self.send().direct(
            &caller,
            &nft_token_id,
            nft_nonce,
            &BigUint::from(NFT_AMOUNT),
            &[],
        );
    }

    // /// private

    fn _mint(&self) -> u64 {
        use alloc::string::ToString;

        // self.require_token_issued();

        let nft_token_id = self.nft_token_id().get();

        let creation_time_key = ManagedBuffer::new_from_bytes(CREATION_TIME_KEY_NAME);
        let creation_time = ManagedBuffer::from(&self.blockchain().get_block_timestamp().to_ne_bytes());
        let mut attributes = ManagedBuffer::new();
        attributes.append(&creation_time_key);
        attributes.append(&creation_time);

        let attributes_hash = self
            .crypto()
            .sha256_legacy(&attributes.to_boxed_bytes().as_slice());
        let hash_buffer = ManagedBuffer::from(attributes_hash.as_bytes());

        let mint_id = self.mint_count().get() + 1;

        let mut name = ManagedBuffer::new();
        name.append(&self.nft_token_name().get());
        name.append(&ManagedBuffer::new_from_bytes(HASH_TAG));
        name.append(&ManagedBuffer::new_from_bytes(mint_id.to_string().as_bytes()));

        sc_print!("name: {:x}", name,);

        let mut uris = ManagedVec::new();
        
        let mut image_uri = ManagedBuffer::new();
        image_uri.append(&self.image_base_uri().get());
        image_uri.append(&ManagedBuffer::new_from_bytes(URI_SLASH));
        image_uri.append(&ManagedBuffer::new_from_bytes(mint_id.to_string().as_bytes()));
        image_uri.append(&ManagedBuffer::new_from_bytes(IMAGE_FILE_EXTENSION));

        sc_print!("name: {:x}", image_uri);

        uris.push(image_uri);

        let mut metadata_uri = ManagedBuffer::new();
        metadata_uri.append(&self.image_base_uri().get());
        metadata_uri.append(&ManagedBuffer::new_from_bytes(URI_SLASH));
        metadata_uri.append(&ManagedBuffer::new_from_bytes(mint_id.to_string().as_bytes()));
        metadata_uri.append(&ManagedBuffer::new_from_bytes(METADATA_FILE_EXTENSION));

        sc_print!("name: {:x}", metadata_uri);

        uris.push(metadata_uri);

        let nft_nonce = self.send().esdt_nft_create(
            &nft_token_id,
            &BigUint::from(NFT_AMOUNT),
            &name,
            &BigUint::from(self.royalties().get()),
            &hash_buffer,
            &attributes,
            &uris,
        );

        self.mint_count().update(|v| *v += 1);

        nft_nonce
    }

    fn require_token_issued(&self) {
        require!(!self.nft_token_id().is_empty(), "Token not issued");
    }

    // callbacks

    #[callback]
    fn issue_callback(&self, #[call_result] result: ManagedAsyncCallResult<TokenIdentifier>) {
        match result {
            ManagedAsyncCallResult::Ok(token_id) => {
                self.nft_token_id().set(&token_id);
            },
            ManagedAsyncCallResult::Err(_) => {
                let caller = self.blockchain().get_owner_address();
                let (returned_tokens, token_id) = self.call_value().payment_token_pair();
                if token_id.is_egld() && returned_tokens > 0 {
                    self.send()
                        .direct(&caller, &token_id, 0, &returned_tokens, &[]);
                }
            },
        }
    }

    /// storage

    #[view(getNftTokenId)]
    #[storage_mapper("nft_token_id")]
    fn nft_token_id(&self) -> SingleValueMapper<TokenIdentifier>;

    #[view(getNftTokenPrice)]
    #[storage_mapper("nft_token_price")]
    fn nft_token_price(&self) -> SingleValueMapper<BigUint>;

    #[view(getPaymentTokenId)]
    #[storage_mapper("payment_token_id")]
    fn payment_token_id(&self) -> SingleValueMapper<TokenIdentifier>;

    #[view(isPaused)]
    #[storage_mapper("paused")]
    fn paused(&self) -> SingleValueMapper<bool>;

    #[view(getMintCount)]
    #[storage_mapper("mint_count")]
    fn mint_count(&self) -> SingleValueMapper<u32>;

    // base metadatas

    #[view(getNftTokenName)]
    #[storage_mapper("nft_token_name")]
    fn nft_token_name(&self) -> SingleValueMapper<ManagedBuffer>;

    #[view(getRoyalties)]
    #[storage_mapper("royalties")]
    fn royalties(&self) -> SingleValueMapper<u32>;

    #[view(getImageBaseUri)]
    #[storage_mapper("image_base_uri")]
    fn image_base_uri(&self) -> SingleValueMapper<ManagedBuffer>;

    #[view(getMetadataBaseUri)]
    #[storage_mapper("metadata_base_uri")]
    fn metadata_base_uri(&self) -> SingleValueMapper<ManagedBuffer>;
}
