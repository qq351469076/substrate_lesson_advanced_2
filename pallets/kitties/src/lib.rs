#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use codec::{Decode, Encode};
	use frame_support::pallet_prelude::*;
	use frame_support::traits::{tokens::ExistenceRequirement, Currency, Randomness};
	use frame_support::transactional;
	use frame_system::pallet_prelude::*;
	use scale_info::TypeInfo;
	use sp_io::hashing::blake2_128;

	type KittyIndex = u32;
	type BalanceOf<T> =
		<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	/// 小猫 基因
	#[derive(Clone, Encode, Decode, PartialEq, RuntimeDebug, TypeInfo)]
	#[scale_info(skip_type_params(T))]
	pub struct Kitty<T: Config> {
		pub dna: [u8; 16],
		pub price: Option<BalanceOf<T>>,
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		// Balance实现
		type Currency: Currency<Self::AccountId>;
		type Randomness: Randomness<Self::Hash, Self::BlockNumber>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub (super) trait Store)]
	pub struct Pallet<T>(_);

	/// 小猫现有数量
	#[pallet::storage]
	#[pallet::getter(fn kitties_count)]
	pub type KittiesCount<T> = StorageValue<_, KittyIndex>;

	/// 小猫索引: 小猫dna
	#[pallet::storage]
	#[pallet::getter(fn kitties)]
	pub type Kitties<T> = StorageMap<_, Blake2_128Concat, KittyIndex, Kitty<T>>;

	/// 小猫索引: Option(主人)
	#[pallet::storage]
	#[pallet::getter(fn owner)]
	pub type Owner<T: Config> = StorageMap<_, Blake2_128Concat, KittyIndex, T::AccountId>;

	#[pallet::event]
	#[pallet::generate_deposit(pub (super) fn deposit_event)]
	pub enum Event<T: Config> {
		KittyCreate(T::AccountId, KittyIndex),
		Transfer(T::AccountId, KittyIndex, T::AccountId),
		BreedSuccess(T::AccountId, KittyIndex, KittyIndex),
		SetPriceSuccess(T::AccountId, KittyIndex, BalanceOf<T>),
		TransferSuccess(T::AccountId, T::AccountId, KittyIndex),
	}

	#[pallet::error]
	pub enum Error<T> {
		KittiesCountOverflow, // 系统预留最大小猫数量溢出
		CanNotYourSelf,       // 调用方不能是自己
		NotOwner,             // 你不是这个小猫的主人
		GenesCanNotSame,      // 小猫的父亲和母亲不能是同一个
		InvalidKittyIndex,    // 不存在这个小猫
		PriceNotZero,         // 售卖价格不能为0
		PriceIsNone,          // 小猫没有设置价格
		MoneyNotEnough,       // 买家的钱不够买小猫
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// 创建小猫
		#[pallet::weight(0)]
		pub fn create(origin: OriginFor<T>) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// 获得 当前小猫id
			let kitty_id = match Self::kitties_count() {
				None => 1,
				Some(index) => {
					ensure!(index != KittyIndex::max_value(), Error::<T>::KittiesCountOverflow);
					index
				}
			};

			// 随机生成小猫DNA
			let dna = Self::gen_dna();

			Kitties::<T>::insert(kitty_id, Kitty::<T> { dna, price: None });
			Owner::<T>::insert(kitty_id, who.clone());
			KittiesCount::<T>::put(kitty_id + 1);

			Self::deposit_event(Event::KittyCreate(who, kitty_id));

			Ok(().into())
		}

		/// 繁殖小猫
		#[pallet::weight(0)]
		pub fn breed(
			origin: OriginFor<T>,
			kitty_id_1: KittyIndex,
			kitty_id_2: KittyIndex,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			// 确保两只小猫 基因 各不相同
			ensure!(kitty_id_1 != kitty_id_2, Error::<T>::GenesCanNotSame);

			// 确保两只小猫 都存在
			let kitty_1 = Self::kitties(kitty_id_1).ok_or(Error::<T>::InvalidKittyIndex)?;
			let kitty_2 = Self::kitties(kitty_id_2).ok_or(Error::<T>::InvalidKittyIndex)?;

			let kitty_id = match Self::kitties_count() {
				None => 1,
				Some(kitty_id) => kitty_id,
			};

			let dna_1 = kitty_1.dna;
			let dna_2 = kitty_2.dna;

			let selector = Self::gen_dna();
			let mut new_dna = [0u8; 16];

			for i in 0..dna_1.len() {
				new_dna[i] = selector[i] & dna_1[i] | (selector[i] & dna_2[i])
			}

			Kitties::<T>::insert(kitty_id, Kitty::<T> { dna: new_dna, price: None });
			Owner::<T>::insert(kitty_id, who.clone());
			KittiesCount::<T>::put(kitty_id + 1);

			Self::deposit_event(Event::BreedSuccess(who, kitty_id_1, kitty_id_2));

			Ok(().into())
		}

		/// 给小猫设置价格（卖）
		#[pallet::weight(0)]
		pub fn set_price(
			origin: OriginFor<T>,
			kitty_id: KittyIndex,
			price: BalanceOf<T>,
		) -> DispatchResult {
			let sender = ensure_signed(origin)?;

			let sender_backup = sender.clone();

			// 检查这只猫是否真实存在
			let mut kitty = Self::kitties(&kitty_id).ok_or(<Error<T>>::InvalidKittyIndex)?;

			// 判断这只猫是否属于此人
			ensure!(Self::owner(&kitty_id) == Some(sender), <Error<T>>::NotOwner);

			// 确保 小猫售价大于0
			ensure!(price > 0u32.into(), <Error<T>>::PriceNotZero);

			kitty.price = Some(price);
			<Kitties<T>>::insert(kitty_id, kitty);

			Self::deposit_event(Event::SetPriceSuccess(sender_backup, kitty_id, price));

			Ok(().into())
		}

		/// 购买小猫
		#[transactional]
		#[pallet::weight(0)]
		pub fn buy_kitty(origin: OriginFor<T>, kitty_id: KittyIndex) -> DispatchResult {
			let buyer = ensure_signed(origin)?;

			// 判断小猫是否存在
			let mut kitty = Self::kitties(&kitty_id).ok_or(<Error<T>>::InvalidKittyIndex)?;

			// 判断小猫是否有售价
			if let Some(price) = kitty.price {
				// 判断买家是否有足够的钱
				ensure!(T::Currency::free_balance(&buyer) >= price, <Error<T>>::MoneyNotEnough);
			} else {
				Err(<Error<T>>::PriceIsNone)?
			}

			// 获得卖家ID
			let seller_id = <Owner<T>>::get(&kitty_id).unwrap();

			// 开始转账
			T::Currency::transfer(
				&buyer,
				&seller_id,
				kitty.price.unwrap(),
				ExistenceRequirement::KeepAlive,
			)?;

			// 更改小猫的主人
			<Owner<T>>::insert(&kitty_id, &buyer);

			// 小猫售价设置为None
			kitty.price = None;
			<Kitties<T>>::insert(&kitty_id, kitty);

			Self::deposit_event(Event::TransferSuccess(buyer.clone(), seller_id.clone(), kitty_id));

			Ok(().into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// 随机生成小猫DNA算法
		fn gen_dna() -> [u8; 16] {
			let payload =
				(T::Randomness::random(&b"dna"[..]).0, <frame_system::Pallet<T>>::block_number());
			payload.using_encoded(blake2_128)
		}
	}
}
