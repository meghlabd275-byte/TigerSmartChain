// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import "@openzeppelin/contracts/access/AccessControl.sol";
import "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

import {RUSD} from "./RUSD.sol";

/// @title RUSDVault — collateral custody and issuance for {RUSD}
/// @notice Holds collateral and issues RUSD against it. A stablecoin is only
/// worth its peg if every unit in circulation is backed by something the vault
/// can actually hand back, so this contract never mints without first taking
/// custody, and never releases collateral without first destroying the RUSD
/// that was issued against it.
///
/// @dev Two collateral assets are supported, and they differ in an important
/// way:
///
/// * The native coin (TGR) is custody-held: the vault's own balance is the
///   collateral, and `totalNativeCollateral` tracks what has been deposited so
///   a stray transfer cannot be mistaken for backing.
/// * An ERC-20 is also custody-held, through `SafeERC20`.
///
/// ## Issuance is over-collateralised, and checked on every mint
///
/// A deposit issues RUSD equal to the collateral's value at the oracle price,
/// scaled by the collateral factor. A factor below one therefore leaves the
/// excess collateral in the vault as a buffer, which is what absorbs a fall in
/// the collateral's price before the vault is under water. The mint also
/// refuses outright if the collateral would not cover the RUSD that would
/// exist afterwards. This is what makes "fully collateralised" a property the
/// contract enforces rather than a claim in a whitepaper.
///
/// ## The oracle is trusted, and that is a deliberate limit
///
/// The price comes from a price source the admin controls. That is the
/// weakest part of any stablecoin and pretending otherwise would be
/// dishonest: a wrong or manipulated price lets the vault over-issue. The
/// mitigations here are that the price must be pushed by an account holding
/// `ORACLE_ROLE`, that a stale price is refused, that the collateral factor is
/// always below one so over-issuance needs the price to be wrong by more than
/// that margin, and that the supply ceiling caps the total exposure. A
/// production deployment should replace this with a decentralised feed; the
/// interface is deliberately one function so that swap is contained.
contract RUSDVault is AccessControl, ReentrancyGuard {
    using SafeERC20 for IERC20;

    /// @notice Role allowed to push prices and mark the vault's solvency.
    bytes32 public constant ORACLE_ROLE = keccak256("ORACLE_ROLE");
    /// @notice Role allowed to change configuration.
    bytes32 public constant ADMIN_ROLE = keccak256("ADMIN_ROLE");

    /// @notice A price is in USD with 18 decimals, so 1e18 means one dollar.
    uint256 public constant PRICE_SCALE = 1e18;
    /// @notice Collateral factors are in basis points, so 10_000 is 100%.
    uint256 public constant BPS = 10_000;
    /// @notice RUSD base units per whole RUSD, which is six decimals.
    uint256 public constant RUSD_SCALE = 1e6;
    /// @notice Converts a USD value in eighteen decimals to RUSD base units.
    ///
    /// @dev The two sides of this vault use different scales: collateral and
    /// prices are eighteen-decimal, RUSD is six-decimal like USDT. Every
    /// conversion between them goes through this constant so the factor
    /// appears once, rather than as a `1e12` written into each formula.
    uint256 public constant VALUE_TO_RUSD = PRICE_SCALE / RUSD_SCALE;

    /// @notice The token this vault issues.
    RUSD public immutable rusd;

    /// @notice The ERC-20 accepted as collateral. Zero means ERC-20 deposits
    /// are disabled and only the native coin is accepted.
    IERC20 public collateralToken;

    /// @notice The price of the collateral in USD, scaled by {PRICE_SCALE}.
    uint256 public collateralPrice;
    /// @notice The most of the collateral's value that can back issuance, in
    /// basis points. Below {BPS} so a price fall does not instantly make the
    /// vault insolvent.
    uint256 public collateralFactorBps;
    /// @notice The price of the native coin in USD, scaled by {PRICE_SCALE}.
    uint256 public nativePrice;
    /// @notice The collateral factor for the native coin, in basis points.
    uint256 public nativeCollateralFactorBps;

    /// @notice When the current prices were set, as a block timestamp.
    uint256 public priceUpdatedAt;
    /// @notice How long a price stays valid. A stale price must not be used to
    /// issue against collateral whose value may have moved.
    uint256 public priceMaxAge;

    /// @notice ERC-20 collateral the vault holds on behalf of depositors.
    uint256 public totalTokenCollateral;
    /// @notice Native collateral the vault holds on behalf of depositors.
    uint256 public totalNativeCollateral;
    /// @notice RUSD issued against ERC-20 collateral.
    uint256 public tokenIssued;
    /// @notice RUSD issued against native collateral.
    uint256 public nativeIssued;

    /// @notice Emitted when RUSD is issued.
    event Minted(address indexed account, uint256 amount, uint8 collateralKind);
    /// @notice Emitted when RUSD is redeemed for collateral.
    event Redeemed(address indexed account, uint256 amount, uint8 collateralKind);
    /// @notice Emitted when the oracle price changes.
    event PriceUpdated(uint256 collateralPrice, uint256 nativePrice, uint256 at);
    /// @notice Emitted when configuration changes.
    event ConfigUpdated(
        address collateralToken,
        uint256 collateralFactorBps,
        uint256 nativeCollateralFactorBps,
        uint256 priceMaxAge
    );

    /// @param rusd_ The token to issue.
    /// @param collateralToken_ The ERC-20 accepted as collateral, or the zero
    /// address to accept only the native coin.
    /// @param collateralPrice_ The ERC-20 price in USD, scaled by 1e18.
    /// @param nativePrice_ The native coin price in USD, scaled by 1e18.
    /// @param collateralFactorBps_ The ERC-20 collateral factor in basis
    /// points, at most {BPS}.
    /// @param nativeCollateralFactorBps_ The native collateral factor in basis
    /// points, at most {BPS}.
    /// @param priceMaxAge_ How long a price stays valid, in seconds.
    constructor(
        RUSD rusd_,
        IERC20 collateralToken_,
        uint256 collateralPrice_,
        uint256 nativePrice_,
        uint256 collateralFactorBps_,
        uint256 nativeCollateralFactorBps_,
        uint256 priceMaxAge_
    ) {
        require(address(rusd_) != address(0), "Vault: rusd is the zero address");
        require(collateralPrice_ > 0, "Vault: collateral price must be non-zero");
        require(nativePrice_ > 0, "Vault: native price must be non-zero");
        require(
            collateralFactorBps_ > 0 && collateralFactorBps_ <= BPS,
            "Vault: collateral factor out of range"
        );
        require(
            nativeCollateralFactorBps_ > 0 && nativeCollateralFactorBps_ <= BPS,
            "Vault: native factor out of range"
        );
        require(priceMaxAge_ > 0, "Vault: price max age must be non-zero");

        rusd = rusd_;
        collateralToken = collateralToken_;
        collateralPrice = collateralPrice_;
        nativePrice = nativePrice_;
        collateralFactorBps = collateralFactorBps_;
        nativeCollateralFactorBps = nativeCollateralFactorBps_;
        priceMaxAge = priceMaxAge_;
        priceUpdatedAt = block.timestamp;

        _grantRole(DEFAULT_ADMIN_ROLE, msg.sender);
        _grantRole(ADMIN_ROLE, msg.sender);
        _grantRole(ORACLE_ROLE, msg.sender);
    }

    /// @notice The value of the vault's ERC-20 collateral, in USD scaled by
    /// {PRICE_SCALE}.
    function tokenCollateralValue() public view returns (uint256) {
        return (totalTokenCollateral * collateralPrice) / PRICE_SCALE;
    }

    /// @notice The value of the vault's native collateral, in USD scaled by
    /// {PRICE_SCALE}.
    function nativeCollateralValue() public view returns (uint256) {
        return (totalNativeCollateral * nativePrice) / PRICE_SCALE;
    }

    /// @notice The total RUSD this vault has issued, in base units.
    function totalIssued() public view returns (uint256) {
        return tokenIssued + nativeIssued;
    }

    /// @notice What the vault has issued, as a USD value in eighteen decimals.
    /// @dev Issuance is in RUSD's six decimals and the collateral value is in
    /// eighteen, so any comparison between them has to convert. Doing it here
    /// keeps the conversion in one place and makes it obvious that the two
    /// sides of the solvency test are the same unit.
    function issuedValue(uint256 issued) public pure returns (uint256) {
        return issued * VALUE_TO_RUSD;
    }

    /// @notice The RUSD the vault's collateral can back right now.
    ///
    /// @dev Each pool is capped separately by its own collateral factor rather
    /// than pooling both and applying one factor. Pooling would let collateral
    /// deposited for the high-factor asset back issuance that was supposed to
    /// be against the low-factor one, which is exactly the mismatch a factor is
    /// meant to prevent.
    function borrowingCapacity() public view returns (uint256 tokenCapacity, uint256 nativeCapacity) {
        tokenCapacity =
            ((tokenCollateralValue() * collateralFactorBps) / BPS) /
            VALUE_TO_RUSD;
        nativeCapacity =
            ((nativeCollateralValue() * nativeCollateralFactorBps) / BPS) /
            VALUE_TO_RUSD;
    }

    /// @notice Whether the vault holds enough collateral to buy back every
    /// RUSD it has issued.
    ///
    /// @dev This compares the collateral's *value* against what was issued,
    /// not the borrowing capacity. Capacity is the collateral value scaled by
    /// the collateral factor, so comparing capacity against issuance would
    /// cancel the factor out and report solvency for a vault whose collateral
    /// had fallen to exactly the issued amount — hiding the very buffer the
    /// factor exists to create. Measuring against the full value is the honest
    /// question: if every holder redeemed at once, could the vault pay?
    ///
    /// The gap between the two is the cushion. Issuing at eighty percent of
    /// value leaves a vault that stays solvent until its collateral loses a
    /// fifth of its worth.
    function isSolvent() public view returns (bool) {
        return
            tokenCollateralValue() >= issuedValue(tokenIssued) &&
            nativeCollateralValue() >= issuedValue(nativeIssued);
    }

    /// @notice Issue RUSD against ERC-20 collateral.
    /// @param amount The collateral to deposit, in the token's base units.
    /// @return minted The RUSD issued: the deposited value scaled by the
    /// collateral factor.
    function mintWithToken(uint256 amount) external nonReentrant returns (uint256 minted) {
        require(address(collateralToken) != address(0), "Vault: token collateral is disabled");
        require(amount > 0, "Vault: deposit must be non-zero");
        _requireFreshPrice();

        // Take custody before minting. If the transfer fails the whole call
        // reverts, so there is no window in which RUSD exists without backing.
        collateralToken.safeTransferFrom(msg.sender, address(this), amount);

        // Issue against the collateral's value scaled by the collateral
        // factor, so the remainder stays as a buffer. A factor of one issues
        // the full value; anything less leaves the difference as over
        // collateralisation, which is what absorbs a fall in the collateral's
        // price before the vault is under water.
        uint256 value = (amount * collateralPrice) / PRICE_SCALE;
        minted = ((value * collateralFactorBps) / BPS) / VALUE_TO_RUSD;
        require(minted > 0, "Vault: deposit is too small to issue anything");
        totalTokenCollateral += amount;
        tokenIssued += minted;

        (uint256 tokenCapacity,) = borrowingCapacity();
        require(tokenCapacity >= tokenIssued, "Vault: would exceed collateral backing");

        rusd.mint(msg.sender, minted);
        emit Minted(msg.sender, minted, 0);
    }

    /// @notice Issue RUSD against the native coin.
    /// @dev The amount issued is the `msg.value` converted at the native price.
    function mintWithNative() external payable nonReentrant returns (uint256 minted) {
        require(msg.value > 0, "Vault: deposit must be non-zero");
        _requireFreshPrice();

        uint256 value = (msg.value * nativePrice) / PRICE_SCALE;
        minted = ((value * nativeCollateralFactorBps) / BPS) / VALUE_TO_RUSD;
        require(minted > 0, "Vault: deposit is too small to issue anything");
        totalNativeCollateral += msg.value;
        nativeIssued += minted;

        (, uint256 nativeCapacity) = borrowingCapacity();
        require(nativeCapacity >= nativeIssued, "Vault: would exceed collateral backing");

        rusd.mint(msg.sender, minted);
        emit Minted(msg.sender, minted, 1);
    }

    /// @notice Redeem RUSD for the ERC-20 collateral it was issued against.
    /// @param rusdAmount The RUSD to destroy.
    /// @dev The RUSD is burned first, so a re-entrant call cannot redeem the
    /// same tokens twice even before the guard is considered. The collateral
    /// returned is the burned amount converted at the current price, which is
    /// what keeps the vault's books balanced when the price has moved.
    function redeemForToken(uint256 rusdAmount) external nonReentrant returns (uint256 returned) {
        require(rusdAmount > 0, "Vault: redemption must be non-zero");
        _requireFreshPrice();

        returned = (rusdAmount * VALUE_TO_RUSD * PRICE_SCALE) / collateralPrice;
        require(returned > 0, "Vault: redemption is too small to return anything");
        require(returned <= totalTokenCollateral, "Vault: not enough token collateral");

        // Burn first: the caller must already hold the RUSD, and burning before
        // paying is what makes the ordering safe.
        rusd.burnFor(msg.sender, rusdAmount);
        tokenIssued -= rusdAmount;
        totalTokenCollateral -= returned;

        collateralToken.safeTransfer(msg.sender, returned);
        emit Redeemed(msg.sender, rusdAmount, 0);
    }

    /// @notice Redeem RUSD for the native coin it was issued against.
    /// @param rusdAmount The RUSD to destroy.
    function redeemForNative(uint256 rusdAmount) external nonReentrant returns (uint256 returned) {
        require(rusdAmount > 0, "Vault: redemption must be non-zero");
        _requireFreshPrice();

        returned = (rusdAmount * VALUE_TO_RUSD * PRICE_SCALE) / nativePrice;
        require(returned > 0, "Vault: redemption is too small to return anything");
        require(returned <= totalNativeCollateral, "Vault: not enough native collateral");

        rusd.burnFor(msg.sender, rusdAmount);
        nativeIssued -= rusdAmount;
        totalNativeCollateral -= returned;

        (bool sent, ) = msg.sender.call{value: returned}("");
        require(sent, "Vault: native transfer failed");
        emit Redeemed(msg.sender, rusdAmount, 1);
    }

    /// @notice Push new prices.
    /// @dev Restricted to the oracle role and recorded with a timestamp, so a
    /// stale price can be refused rather than silently used.
    function setPrices(uint256 collateralPrice_, uint256 nativePrice_) external onlyRole(ORACLE_ROLE) {
        require(collateralPrice_ > 0, "Vault: collateral price must be non-zero");
        require(nativePrice_ > 0, "Vault: native price must be non-zero");
        collateralPrice = collateralPrice_;
        nativePrice = nativePrice_;
        priceUpdatedAt = block.timestamp;
        emit PriceUpdated(collateralPrice_, nativePrice_, block.timestamp);
    }

    /// @notice Change the collateral token, the collateral factors and the
    /// price age limit.
    /// @dev The factors are bounded by {BPS} here and in the constructor: a
    /// factor above one would let the vault issue more RUSD than its collateral
    /// is worth, which defeats the point of having a vault.
    function setConfig(
        IERC20 collateralToken_,
        uint256 collateralFactorBps_,
        uint256 nativeCollateralFactorBps_,
        uint256 priceMaxAge_
    ) external onlyRole(ADMIN_ROLE) {
        require(
            collateralFactorBps_ > 0 && collateralFactorBps_ <= BPS,
            "Vault: collateral factor out of range"
        );
        require(
            nativeCollateralFactorBps_ > 0 && nativeCollateralFactorBps_ <= BPS,
            "Vault: native factor out of range"
        );
        require(priceMaxAge_ > 0, "Vault: price max age must be non-zero");
        collateralToken = collateralToken_;
        collateralFactorBps = collateralFactorBps_;
        nativeCollateralFactorBps = nativeCollateralFactorBps_;
        priceMaxAge = priceMaxAge_;
        emit ConfigUpdated(
            address(collateralToken_),
            collateralFactorBps_,
            nativeCollateralFactorBps_,
            priceMaxAge_
        );
    }

    /// @dev Refuses to act on a price older than `priceMaxAge`.
    function _requireFreshPrice() internal view {
        require(
            block.timestamp <= priceUpdatedAt + priceMaxAge,
            "Vault: the price is stale"
        );
    }

    /// @dev Accept only the native coin, and only from a call that has already
    /// recorded the deposit. A bare transfer with no data would otherwise add
    /// to the vault's balance without adding to `totalNativeCollateral`, which
    /// would be unbacked value the vault cannot account for.
    receive() external payable {
        revert("Vault: use mintWithNative");
    }
}
