# Tool reference

All **113** tools exposed by `appstore-mcp`, grouped by domain. Auto-generated from the server's live `tools/list` schemas by `scripts/gen_tools_doc.py` — regenerate after changing tools.

> Required parameters are marked **yes**. IDs are opaque strings returned by the `list_*`/`get_*` tools — resolve them first. Anything not covered here is reachable via the generic `appstore_request` / `appstore_list` tools.

## Contents

- [Generic](#generic) (2)
- [Apps & metadata](#apps--metadata) (8)
- [In-app purchases (v2)](#in-app-purchases-v2) (7)
- [Subscriptions](#subscriptions) (6)
- [Versions & metadata](#versions--metadata) (4)
- [App Review submission](#app-review-submission) (8)
- [Pricing](#pricing) (3)
- [Availability](#availability) (3)
- [TestFlight](#testflight) (10)
- [Provisioning & signing](#provisioning--signing) (10)
- [Assets](#assets) (7)
- [Subscription offers](#subscription-offers) (4)
- [Offer codes](#offer-codes) (4)
- [Promoted purchases](#promoted-purchases) (4)
- [Customer reviews](#customer-reviews) (3)
- [Phased release](#phased-release) (2)
- [Users & access](#users--access) (4)
- [In-app events](#in-app-events) (3)
- [Xcode Cloud](#xcode-cloud) (5)
- [Analytics reports](#analytics-reports) (4)
- [Custom product pages](#custom-product-pages) (12)

## Generic

Reach any endpoint with raw JSON:API.

### `appstore_request`

Make a raw authenticated request to ANY App Store Connect API endpoint (method + path + optional query + optional JSON:API body). Use this for operations without a dedicated tool. Returns the parsed JSON response.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `method` | string | **yes** | HTTP method: GET, POST, PATCH, PUT, or DELETE. |
| `path` | string | **yes** | API path or full URL, e.g. "/v1/apps", "v2/inAppPurchases/{id}", or a `next` link returned by a previous list call. |
| `body` | object | no | Optional JSON:API request body for POST/PATCH/PUT — the full document, e.g. {"data": {"type": "apps", "id": "123", "attributes": {...}}}. |
| `query` | object | no | Optional query parameters, e.g. {"filter[bundleId]": "com.example.app", "limit": 50}. Array values are comma-joined. |

### `appstore_list`

List any App Store Connect collection with optional filters, sort, include, and pagination. Returns one page; pass the `next` link (from the response's links.next) back as `cursor` to fetch subsequent pages.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `path` | string | **yes** | Collection path or full URL to GET, e.g. "/v1/apps". |
| `cursor` | string | no | Opaque pagination cursor from a previous response's `data.links.next`. |
| `filters` | object | no | Optional filters, e.g. {"filter[name]": "MyApp"}. |
| `include` | string | no | Comma-separated related resources to include, e.g. "appStoreVersions". |
| `limit` | integer | no | Page size (App Store Connect maximum is 200). Sparse-fieldset selections (`fields[...]`) can be passed via `filters` if needed. |
| `sort` | string | no | Comma-separated sort keys, e.g. "-createdDate". |

## Apps & metadata

Read/update apps, app-level metadata, age rating, and localized app name/subtitle.

### `list_apps`

List apps on the account, optionally filtered by bundle ID, name, or SKU.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `bundle_id` | string | no | Filter by bundle ID, e.g. "com.example.app". |
| `limit` | integer | no | Page size (max 200). |
| `name` | string | no | Filter by app name. |
| `sku` | string | no | Filter by SKU. |

### `get_app`

Get a single app by its App Store Connect ID, with optional includes.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `include` | string | no | Comma-separated related resources to include, e.g. "appInfos,appStoreVersions". |

### `update_app`

Update an app's attributes (e.g. primaryLocale, availableInNewTerritories, contentRightsDeclaration).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `attributes` | object | **yes** | Attributes to update, e.g. {"primaryLocale": "en-US", "availableInNewTerritories": true}. |

### `list_app_infos`

List an app's appInfos — metadata containers holding category and age-rating relationships for the app.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |

### `update_app_info`

Update an appInfo's attributes by appInfo ID.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_info_id` | string | **yes** | The appInfo ID (from list_app_infos). |
| `attributes` | object | **yes** | Attributes to update, e.g. category/content-rights relationships are set separately. |

### `set_age_rating`

Set an app's age-rating questionnaire answers (required before submission). Pass the ageRatingDeclaration ID and the questionnaire attributes. Enum values are typically NONE / INFREQUENT_OR_MILD / FREQUENT_OR_INTENSE, plus booleans for items like gambling and unrestrictedWebAccess.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `age_rating_declaration_id` | string | **yes** | The ageRatingDeclaration ID (from appInfo's ageRatingDeclaration relationship — fetch via get_app or list_app_infos with include=ageRatingDeclaration). |
| `attributes` | object | **yes** | Questionnaire answers, e.g. {"violenceCartoonOrFantasy": "NONE", "gamblingSimulated": "FREQUENT_OR_INTENSE", "unrestrictedWebAccess": false}. |

### `create_app_info_localization`

Create a localized app name, subtitle, and privacy policy for a locale (appInfoLocalizations). This is the app-level name/subtitle, distinct from per-version metadata.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_info_id` | string | **yes** | The appInfo ID (from list_app_infos). |
| `locale` | string | **yes** | BCP-47 locale, e.g. "en-US". |
| `name` | string | **yes** | Localized app name (required by the API). To change only privacy fields on an existing localization, use update_app_info_localization instead. |
| `privacy_choices_url` | string | no | Privacy choices URL. |
| `privacy_policy_text` | string | no | Privacy policy text (for platforms that show inline text, e.g. tvOS). |
| `privacy_policy_url` | string | no | Privacy policy URL. |
| `subtitle` | string | no | Localized subtitle. |

### `update_app_info_localization`

Update an appInfoLocalization by ID (name, subtitle, privacy URLs/text).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `attributes` | object | **yes** | Attributes to update (name, subtitle, privacyPolicyUrl, privacyPolicyText, privacyChoicesUrl). |
| `localization_id` | string | **yes** | The appInfoLocalization ID. |

## In-app purchases (v2)

Create and manage non-subscription in-app purchases.

### `list_in_app_purchases`

List an app's in-app purchases (IAP v2), optionally filtered by productId.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `include` | string | no | Comma-separated includes, e.g. "inAppPurchaseLocalizations,iapPriceSchedule". |
| `limit` | integer | no | Page size (max 200). |
| `product_id` | string | no | Filter by exact productId. |

### `create_in_app_purchase`

Create an in-app purchase (v2): provide a reference name, productId, and type (CONSUMABLE, NON_CONSUMABLE, or NON_RENEWING_SUBSCRIPTION). Add localizations and a price afterward.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `iap_type` | `CONSUMABLE \| NON_CONSUMABLE \| NON_RENEWING_SUBSCRIPTION` | **yes** | Purchase type. |
| `name` | string | **yes** | Reference name shown in App Store Connect (not visible to customers). |
| `product_id` | string | **yes** | Unique product ID, e.g. "com.example.app.coins_100". |
| `available_in_all_territories` | boolean | no | Whether to make it available in all current and future territories. |
| `family_sharable` | boolean | no | Whether the purchase is family-sharable. |
| `review_note` | string | no | Optional note for App Review. |

### `update_in_app_purchase`

Update an in-app purchase's attributes (name, reviewNote, familySharable, etc.).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `attributes` | object | **yes** | Attributes to update, e.g. {"name": "...", "reviewNote": "...", "familySharable": true}. |
| `iap_id` | string | **yes** | The in-app purchase ID. |

### `delete_in_app_purchase`

Delete an in-app purchase by ID (only allowed before it is approved).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `iap_id` | string | **yes** | The in-app purchase ID. |

### `create_iap_localization`

Add a localized name/description to an in-app purchase for a given locale (e.g. en-US).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `iap_id` | string | **yes** | The in-app purchase ID. |
| `locale` | string | **yes** | BCP-47 locale, e.g. "en-US". |
| `name` | string | **yes** | Display name shown to customers. |
| `description` | string | no | Optional customer-facing description. |

### `set_iap_price_schedule`

Set an in-app purchase's price by creating a price schedule from a price point (look up the price_point_id with list_iap_price_points). Defaults to base territory USA, effective immediately.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `iap_id` | string | **yes** | The in-app purchase ID. |
| `price_point_id` | string | **yes** | The inAppPurchasePricePoint ID (look up with list_iap_price_points). |
| `base_territory` | string | no | Base territory for the price schedule (default "USA"). |
| `start_date` | string | no | Optional ISO-8601 start date (YYYY-MM-DD). Null/absent means effective immediately. |

### `upload_iap_review_screenshot`

Upload an App Store review screenshot for an in-app purchase (reserve → upload → commit, with MD5 verification). Provide a local image file path.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `file_path` | string | **yes** | Local path to the review screenshot image file (PNG/JPEG). |
| `iap_id` | string | **yes** | The in-app purchase ID. |

## Subscriptions

Subscription groups, subscriptions, localizations, and prices.

### `list_subscription_groups`

List an app's subscription groups.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `include` | string | no | Comma-separated includes, e.g. "subscriptions". |

### `create_subscription_group`

Create a subscription group for an app (subscriptions live inside a group).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `reference_name` | string | **yes** | Reference name for the subscription group (not customer-facing). |

### `create_subscription`

Create an auto-renewable subscription inside a group: reference name, productId, and renewal period (ONE_WEEK..ONE_YEAR).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `group_id` | string | **yes** | The subscription group ID to add this subscription to. |
| `name` | string | **yes** | Reference name (not customer-facing). |
| `product_id` | string | **yes** | Unique product ID, e.g. "com.example.app.pro_monthly". |
| `subscription_period` | `ONE_WEEK \| ONE_MONTH \| TWO_MONTHS \| THREE_MONTHS \| SIX_MONTHS \| ONE_YEAR` | **yes** | Renewal period. |
| `family_sharable` | boolean | no | Whether the subscription is family-sharable. |
| `group_level` | integer | no | Rank within the group (1 = highest level/most features). Defaults to 1. |

### `update_subscription`

Update a subscription's attributes (name, groupLevel, familySharable, etc.).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `attributes` | object | **yes** | Attributes to update, e.g. {"name": "...", "groupLevel": 2}. |
| `subscription_id` | string | **yes** | The subscription ID. |

### `create_subscription_localization`

Add a localized name/description to a subscription for a given locale.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `locale` | string | **yes** | BCP-47 locale, e.g. "en-US". |
| `name` | string | **yes** | Display name shown to customers. |
| `subscription_id` | string | **yes** | The subscription ID. |
| `description` | string | no | Optional customer-facing description. |

### `set_subscription_price`

Set a subscription's price from a price point in a territory (look up the price_point_id with list_subscription_price_points). Defaults to territory USA.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `price_point_id` | string | **yes** | The subscriptionPricePoint ID (look up with list_subscription_price_points). |
| `subscription_id` | string | **yes** | The subscription ID. |
| `preserve_current_price` | boolean | no | Preserve the current price for existing subscribers (no price increase consent). |
| `start_date` | string | no | Optional ISO-8601 start date (YYYY-MM-DD). Absent means effective immediately. |
| `territory` | string | no | Territory ID (default "USA"). |

## Versions & metadata

App Store versions and their localized metadata.

### `list_app_store_versions`

List an app's App Store versions, optionally filtered by state or platform.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `include` | string | no | Comma-separated includes, e.g. "appStoreVersionLocalizations,build". |
| `platform` | string | no | Filter by platform, e.g. "IOS". |
| `state` | string | no | Filter by version state, e.g. "PREPARE_FOR_SUBMISSION", "READY_FOR_SALE". |

### `create_app_store_version`

Create a new App Store version for an app (platform + version string, with optional release type and copyright).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `platform` | `IOS \| MAC_OS \| TV_OS \| VISION_OS` | **yes** | Target platform. |
| `version_string` | string | **yes** | Version string, e.g. "1.2.0". |
| `copyright` | string | no | Optional copyright string. |
| `release_type` | string | no | Optional release type: "MANUAL", "AFTER_APPROVAL", or "SCHEDULED". |

### `create_version_localization`

Create localized App Store metadata (description, keywords, whatsNew, URLs) for a version + locale.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `locale` | string | **yes** | BCP-47 locale, e.g. "en-US". |
| `version_id` | string | **yes** | The appStoreVersion ID. |
| `description` | string | no | Optional fields: description, keywords, whatsNew, promotionalText, marketingUrl, supportUrl. |
| `keywords` | string | no |  |
| `marketing_url` | string | no |  |
| `promotional_text` | string | no |  |
| `support_url` | string | no |  |
| `whats_new` | string | no |  |

### `update_version_localization`

Update an App Store version localization by ID (description, keywords, whatsNew, URLs).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `attributes` | object | **yes** | Attributes to update (description, keywords, whatsNew, promotionalText, marketingUrl, supportUrl). |
| `localization_id` | string | **yes** | The appStoreVersionLocalization ID. |

## App Review submission

Submit versions/IAPs for review and satisfy the metadata gates.

### `create_review_submission`

Open a new App Review submission for an app + platform. Then attach items with add_review_submission_item and submit with submit_review_submission.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `platform` | `IOS \| MAC_OS \| TV_OS \| VISION_OS` | **yes** | Target platform. |

### `add_review_submission_item`

Attach an App Store version or in-app event to an open review submission.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `item_id` | string | **yes** | The ID of the version/event being submitted. |
| `item_kind` | object | **yes** | What kind of item to attach. |
| `review_submission_id` | string | **yes** | The review submission ID (from create_review_submission). |

### `submit_review_submission`

Submit a prepared review submission to App Review (sets submitted=true). All metadata gates (age rating, export compliance, review details) must be satisfied first.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `review_submission_id` | string | **yes** | The review submission ID. |

### `list_review_submissions`

List an app's App Review submissions, optionally filtered by state or platform.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `platform` | string | no | Filter by platform, e.g. "IOS". |
| `state` | string | no | Filter by state, e.g. "READY_FOR_REVIEW", "WAITING_FOR_REVIEW", "IN_REVIEW". |

### `submit_in_app_purchase`

Submit a single in-app purchase for App Review (inAppPurchaseSubmissions).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `iap_id` | string | **yes** | The in-app purchase ID to submit for review. |

### `set_app_review_detail`

Set the App Review contact info, optional demo account, and notes for a version. Pass review_detail_id to update an existing detail; omit it to create one.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `version_id` | string | **yes** | The appStoreVersion ID the review details belong to. |
| `contact_email` | string | no |  |
| `contact_first_name` | string | no |  |
| `contact_last_name` | string | no |  |
| `contact_phone` | string | no |  |
| `demo_account_name` | string | no |  |
| `demo_account_password` | string | no |  |
| `demo_account_required` | boolean | no | Whether App Review needs a demo account to use the app. |
| `notes` | string | no | Free-form notes for the reviewer. |
| `review_detail_id` | string | no | If updating an existing detail, its ID (PATCH instead of POST). |

### `create_app_encryption_declaration`

Create an app encryption / export-compliance declaration for an app. Then attach a build with assign_build_encryption_declaration.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_description` | string | **yes** | A description of how the app uses encryption. |
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `available_on_french_store` | boolean | **yes** | Whether the app will be available on the French App Store. |
| `contains_proprietary_cryptography` | boolean | **yes** | Whether the app implements any proprietary/non-standard encryption algorithms. |
| `contains_third_party_cryptography` | boolean | **yes** | Whether the app uses any third-party encryption. |

### `assign_build_encryption_declaration`

Associate a build with an existing app encryption declaration.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `build_id` | string | **yes** | The build ID to associate with the declaration. |
| `declaration_id` | string | **yes** | The appEncryptionDeclaration ID. |

## Pricing

Territories and price-point lookups.

### `list_territories`

List App Store territories (territory IDs like "USA", "GBR", used for pricing).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `limit` | integer | no | Page size (max 200). Defaults to 200 to return all territories in one page. |

### `list_iap_price_points`

List the available price points for an in-app purchase (each has an id and customerPrice). Use the id with set_iap_price_schedule. Filter by territory to narrow results.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `iap_id` | string | **yes** | The in-app purchase ID. |
| `limit` | integer | no | Page size (max 200). |
| `territory` | string | no | Filter to a single territory, e.g. "USA". Recommended to keep results small. |

### `list_subscription_price_points`

List the available price points for a subscription (each has an id and customerPrice). Use the id with set_subscription_price. Filter by territory to narrow results.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `subscription_id` | string | **yes** | The subscription ID. |
| `limit` | integer | no | Page size (max 200). |
| `territory` | string | no | Filter to a single territory, e.g. "USA". Recommended to keep results small. |

## Availability

Control which territories products/apps are sold in.

### `set_iap_availability`

Set the territories an in-app purchase is available in (territory IDs from list_territories).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `available_in_new_territories` | boolean | **yes** | Whether to make it available in future new territories automatically. Required by the API. |
| `product_id` | string | **yes** | The product ID (in-app purchase ID or subscription ID). |
| `territory_ids` | array of string | **yes** | Territory IDs to make the product available in, e.g. ["USA", "GBR"]. |

### `set_subscription_availability`

Set the territories a subscription is available in (territory IDs from list_territories).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `available_in_new_territories` | boolean | **yes** | Whether to make it available in future new territories automatically. Required by the API. |
| `product_id` | string | **yes** | The product ID (in-app purchase ID or subscription ID). |
| `territory_ids` | array of string | **yes** | Territory IDs to make the product available in, e.g. ["USA", "GBR"]. |

### `set_app_availability`

Set the territories an app is available in (territory IDs from list_territories). Uses the App Availability v2 API.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `available_in_new_territories` | boolean | **yes** | Whether to make it available in future new territories automatically. Required by the API. |
| `territory_ids` | array of string | **yes** | Territory IDs to make the app available in, e.g. ["USA", "GBR"]. |

## TestFlight

Builds, beta groups, testers, beta review, and build details.

### `list_builds`

List TestFlight builds, optionally filtered by app or build version.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | no | Filter by app ID. |
| `include` | string | no | Comma-separated includes, e.g. "betaGroups,preReleaseVersion". |
| `limit` | integer | no | Page size (max 200). |
| `version` | string | no | Filter by version (build number), e.g. "42". |

### `list_beta_groups`

List TestFlight beta groups, optionally filtered by app.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | no | Filter by app ID. |
| `limit` | integer | no | Page size (max 200). |

### `create_beta_group`

Create a TestFlight beta group for an app.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `name` | string | **yes** | Group name. |
| `public_link_enabled` | boolean | no | Whether this is a public-link group. |

### `add_beta_tester`

Add a beta tester (by email) to a TestFlight beta group, sending an invite.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `beta_group_id` | string | **yes** | The beta group ID to add the tester to. |
| `email` | string | **yes** | Tester email address. |
| `first_name` | string | no | Tester first name. |
| `last_name` | string | no | Tester last name. |

### `submit_build_for_beta_review`

Submit a build for TestFlight beta app review (required before external testing).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `build_id` | string | **yes** | The build ID to submit for beta (external) review. |

### `set_build_test_notes`

Set the TestFlight 'What's New' test notes for a build in a specific locale (creates a betaBuildLocalization). locale is required (e.g. "en-US"); whats_new is the tester-facing 'What to Test' text shown in the TestFlight app. To update an existing localization instead of creating one, use appstore_request with PATCH /v1/betaBuildLocalizations/{id}.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `build_id` | string | **yes** | The build ID to attach test notes to. |
| `locale` | string | **yes** | BCP 47 locale code, e.g. "en-US". |
| `whats_new` | string | no | What's new / test notes shown to testers in TestFlight. |

### `set_build_beta_detail`

Update the beta detail for a build, e.g. toggle auto-notify. The build_beta_detail_id is the buildBetaDetail resource ID — find it via GET /v1/builds/{buildId}/buildBetaDetail or by including ?include=buildBetaDetail on a build fetch.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `build_beta_detail_id` | string | **yes** | The buildBetaDetail resource ID (NOT the build ID). Find it via GET /v1/builds/{buildId}/buildBetaDetail or ?include=buildBetaDetail. |
| `auto_notify_enabled` | boolean | no | Whether to automatically notify testers when the build becomes available. |

### `set_beta_app_review_detail`

Update the TestFlight beta app review contact info, demo account, and notes for an app. beta_app_review_detail_id is the betaAppReviewDetail resource ID — find it via GET /v1/apps/{appId}/betaAppReviewDetail. Only provided fields are sent.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `beta_app_review_detail_id` | string | **yes** | The betaAppReviewDetail resource ID (NOT the app/build ID). Find it via GET /v1/apps/{appId}/betaAppReviewDetail. |
| `contact_email` | string | no | Email address of the beta review contact. |
| `contact_first_name` | string | no | First name of the beta review contact. |
| `contact_last_name` | string | no | Last name of the beta review contact. |
| `contact_phone` | string | no | Phone number of the beta review contact. |
| `demo_account_name` | string | no | Demo account username (if demo account is required). |
| `demo_account_password` | string | no | Demo account password (if demo account is required). |
| `demo_account_required` | boolean | no | Whether App Review requires a demo account to test the app. |
| `notes` | string | no | Free-form notes for the beta reviewer. |

### `expire_build`

Mark a TestFlight build as expired so it is no longer available to testers.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `build_id` | string | **yes** | The build ID to expire. |

### `add_build_to_beta_group`

Add a build to a TestFlight beta group (makes it available for that group's testers). Uses the betaGroups/{id}/relationships/builds to-many endpoint.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `beta_group_id` | string | **yes** | The beta group ID to add the build to. |
| `build_id` | string | **yes** | The build ID to add to the group. |

## Provisioning & signing

Bundle IDs (+ capabilities), certificates, devices, profiles.

### `list_bundle_ids`

List registered bundle IDs.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `include` | string | no | Comma-separated includes. |
| `limit` | integer | no | Page size (max 200). |

### `create_bundle_id`

Register a new bundle ID (name, reverse-DNS identifier, platform).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `identifier` | string | **yes** | The reverse-DNS identifier, e.g. "com.example.app". |
| `name` | string | **yes** | Display name for the bundle ID. |
| `platform` | string | **yes** | Platform: "IOS", "MAC_OS", or "UNIVERSAL". |
| `seed_id` | string | no | Optional team/seed ID. |

### `enable_bundle_id_capability`

Enable a capability on a registered bundle ID. Provide the bundle_id resource ID and the capability_type (e.g. PUSH_NOTIFICATIONS, ICLOUD, APP_GROUPS, ASSOCIATED_DOMAINS, SIGN_IN_WITH_APPLE). Pass settings only for capabilities that require extra configuration (e.g. iCloud containers).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `bundle_id` | string | **yes** | The bundle ID resource ID to enable the capability on. |
| `capability_type` | string | **yes** | The capability type to enable. Common values: PUSH_NOTIFICATIONS, APP_GROUPS, ICLOUD, ASSOCIATED_DOMAINS, IN_APP_PURCHASE, GAME_CENTER, SIGN_IN_WITH_APPLE, MAPS, WALLET, HEALTHKIT, HOMEKIT, WIRELESS_ACCESSORY_CONFIGURATION, DATA_PROTECTION, SIRIKIT, NETWORK_EXTENSIONS, MULTIPATH, HOT_SPOT, NFC_TAG_READING, CLASSKIT, AUTOFILL_CREDENTIAL_PROVIDER, ACCESS_WIFI_INFORMATION, COREMEDIA_HLS_LOW_LATENCY, FONT_INSTALLATION, EXTENDED_VIRTUAL_ADDRESS_SPACE, USER_MANAGEMENT, APPLE_ID_AUTH. |
| `settings` | object | no | Optional array of capability-setting objects (free-form JSON array). Only send when the capability requires settings (e.g. iCloud containers, App Groups identifiers). Structure matches Apple's `CapabilitySetting` model. |

### `disable_bundle_id_capability`

Disable a capability on a bundle ID by deleting the bundleIdCapabilities resource. Pass the capability_id returned from enable_bundle_id_capability or list_bundle_ids (include=bundleIdCapabilities).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `capability_id` | string | **yes** | The bundleIdCapabilities resource ID to delete. |

### `list_certificates`

List signing certificates.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `include` | string | no | Comma-separated includes. |
| `limit` | integer | no | Page size (max 200). |

### `create_certificate`

Create a signing certificate from a CSR (certificate type + PEM CSR content).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `certificate_type` | string | **yes** | Certificate type, e.g. "IOS_DISTRIBUTION", "IOS_DEVELOPMENT", "DISTRIBUTION". |
| `csr_content` | string | **yes** | PEM-encoded Certificate Signing Request (CSR) content. |

### `list_devices`

List registered devices.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `include` | string | no | Comma-separated includes. |
| `limit` | integer | no | Page size (max 200). |

### `register_device`

Register a device for development/ad-hoc distribution (name, platform, UDID).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `name` | string | **yes** | Device name. |
| `platform` | string | **yes** | Platform: "IOS" or "MAC_OS". |
| `udid` | string | **yes** | The device UDID. |

### `list_profiles`

List provisioning profiles.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `include` | string | no | Comma-separated includes. |
| `limit` | integer | no | Page size (max 200). |

### `create_profile`

Create a provisioning profile (name, type, bundle ID, certificate IDs, and device IDs for development/ad-hoc profiles).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `bundle_id` | string | **yes** | The bundle ID resource ID this profile is for. |
| `certificate_ids` | array of string | **yes** | Certificate resource IDs to include. |
| `name` | string | **yes** | Profile name. |
| `profile_type` | string | **yes** | Profile type, e.g. "IOS_APP_DEVELOPMENT", "IOS_APP_STORE", "IOS_APP_ADHOC". |
| `device_ids` | array of string | no | Device resource IDs to include (required for development/ad-hoc profiles). |

## Assets

Screenshot/preview sets and uploads (reserve -> upload -> commit).

### `upload_app_screenshot`

Upload an app screenshot into an appScreenshotSet (reserve → upload → commit with MD5 verification). Provide the set ID and a local image path.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `file_path` | string | **yes** | Local path to the screenshot image file (PNG/JPEG). |
| `screenshot_set_id` | string | **yes** | The appScreenshotSet ID to add this screenshot to. |

### `upload_app_preview`

Upload an app preview video into an appPreviewSet (reserve → upload → commit with MD5 verification). Provide the set ID and a local video path.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `file_path` | string | **yes** | Local path to the preview video file. |
| `preview_set_id` | string | **yes** | The appPreviewSet ID to add this preview to. |

### `create_screenshot_set`

Create an appScreenshotSet for a version localization + display type (e.g. APP_IPHONE_67). Upload screenshots into it with upload_app_screenshot.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `screenshot_display_type` | string | **yes** | Display type, e.g. "APP_IPHONE_67", "APP_IPAD_PRO_129", "APP_WATCH_ULTRA". |
| `version_localization_id` | string | **yes** | The appStoreVersionLocalization ID this set belongs to. |

### `create_preview_set`

Create an appPreviewSet for a version localization + preview type (e.g. IPHONE_67). Upload previews into it with upload_app_preview.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `preview_type` | string | **yes** | Preview type, e.g. "IPHONE_67", "IPAD_PRO_129". |
| `version_localization_id` | string | **yes** | The appStoreVersionLocalization ID this set belongs to. |

### `delete_screenshot_set`

Delete an appScreenshotSet (and its screenshots) by ID.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `set_id` | string | **yes** | The set ID to delete. |

### `delete_preview_set`

Delete an appPreviewSet (and its previews) by ID.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `set_id` | string | **yes** | The set ID to delete. |

### `reorder_screenshots`

Set the display order of screenshots within an appScreenshotSet by passing the screenshot IDs in the desired order.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `ordered_screenshot_ids` | array of string | **yes** | The screenshot IDs in the desired display order. |
| `set_id` | string | **yes** | The appScreenshotSet ID. |

## Subscription offers

Introductory, promotional, and win-back offers.

### `create_introductory_offer`

Create a subscription introductory offer (free trial / pay-as-you-go / pay-up-front). For paid modes supply price_point_id + territory; for FREE_TRIAL omit them.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `duration` | `THREE_DAYS \| ONE_WEEK \| TWO_WEEKS \| ONE_MONTH \| TWO_MONTHS \| THREE_MONTHS \| SIX_MONTHS \| ONE_YEAR` | **yes** | Offer duration (one period's length). |
| `number_of_periods` | integer | **yes** | Number of periods the offer applies for. |
| `offer_mode` | `PAY_AS_YOU_GO \| PAY_UP_FRONT \| FREE_TRIAL` | **yes** | Charge mode. |
| `subscription_id` | string | **yes** | The subscription ID. |
| `end_date` | string | no | Optional ISO-8601 end date (YYYY-MM-DD). |
| `price_point_id` | string | no | Price-point ID — required for PAY_AS_YOU_GO / PAY_UP_FRONT; omit for FREE_TRIAL. |
| `start_date` | string | no | Optional ISO-8601 start date (YYYY-MM-DD). |
| `territory` | string | no | Territory for the price point (e.g. "USA"); pair with price_point_id. |

### `create_promotional_offer`

Create a subscription promotional offer with a code and one price per territory. Look up price-point IDs with list_subscription_price_points.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `duration` | `THREE_DAYS \| ONE_WEEK \| TWO_WEEKS \| ONE_MONTH \| TWO_MONTHS \| THREE_MONTHS \| SIX_MONTHS \| ONE_YEAR` | **yes** | Offer duration. |
| `name` | string | **yes** | Reference name (not customer-facing). |
| `number_of_periods` | integer | **yes** | Number of periods. |
| `offer_code` | string | **yes** | The promotional offer code identifier (used by your app to invoke the offer). |
| `offer_mode` | `PAY_AS_YOU_GO \| PAY_UP_FRONT \| FREE_TRIAL` | **yes** | Charge mode. |
| `prices` | array of OfferPrice | **yes** | One price per territory (at least one required). |
| `subscription_id` | string | **yes** | The subscription ID. |

### `create_winback_offer`

Create a win-back offer to re-acquire churned subscribers (iOS 18+): eligibility windows, priority, and one price per territory.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `duration` | `THREE_DAYS \| ONE_WEEK \| TWO_WEEKS \| ONE_MONTH \| TWO_MONTHS \| THREE_MONTHS \| SIX_MONTHS \| ONE_YEAR` | **yes** | Offer duration. |
| `offer_id` | string | **yes** | The offer identifier used by your app/StoreKit. |
| `offer_mode` | `PAY_AS_YOU_GO \| PAY_UP_FRONT \| FREE_TRIAL` | **yes** | Charge mode. |
| `paid_subscription_duration_in_months` | integer | **yes** | Eligibility: minimum total months the customer previously paid. |
| `period_count` | integer | **yes** | Number of periods. |
| `prices` | array of OfferPrice | **yes** | One price per territory (at least one required). |
| `priority` | `HIGH \| NORMAL` | **yes** | Offer priority. |
| `reference_name` | string | **yes** | Reference name (not customer-facing). |
| `start_date` | string | **yes** | ISO-8601 start date (YYYY-MM-DD), required. |
| `subscription_id` | string | **yes** | The subscription ID. |
| `end_date` | string | no | Optional ISO-8601 end date. |
| `months_since_last_subscribed_max` | integer | no | Eligibility: maximum months since the customer last subscribed. |
| `months_since_last_subscribed_min` | integer | no | Eligibility: minimum months since the customer last subscribed. |
| `promotion_intent` | object | no | Whether/how the offer is promoted. |
| `wait_between_offers_in_months` | integer | no | Eligibility: minimum months to wait between offers. |

### `list_winback_offers`

List the win-back offers configured for a subscription.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `subscription_id` | string | **yes** | The subscription ID. |
| `limit` | integer | no | Page size (max 200). |

## Offer codes

Offer-code campaigns plus one-time-use and custom codes.

### `create_offer_code`

Create a subscription offer-code campaign (eligibility, duration, mode, and one price per territory). Then generate codes with generate_one_time_use_codes or create_custom_offer_code.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `customer_eligibilities` | array of `NEW \| EXISTING \| EXPIRED` | **yes** | Which customers are eligible (at least one): NEW, EXISTING, EXPIRED. |
| `duration` | `THREE_DAYS \| ONE_WEEK \| TWO_WEEKS \| ONE_MONTH \| TWO_MONTHS \| THREE_MONTHS \| SIX_MONTHS \| ONE_YEAR` | **yes** | Offer duration. |
| `name` | string | **yes** | Reference name (not customer-facing). |
| `number_of_periods` | integer | **yes** | Number of periods. |
| `offer_eligibility` | `STACK_WITH_INTRO_OFFERS \| REPLACE_INTRO_OFFERS` | **yes** | How the offer interacts with introductory offers. |
| `offer_mode` | `PAY_AS_YOU_GO \| PAY_UP_FRONT \| FREE_TRIAL` | **yes** | Charge mode. |
| `prices` | array of OfferPrice | **yes** | One price per territory (at least one required). |
| `subscription_id` | string | **yes** | The subscription ID. |

### `generate_one_time_use_codes`

Generate a batch of one-time-use codes for an offer-code campaign. The response includes a values URL to download the codes.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `expiration_date` | string | **yes** | Expiration date (YYYY-MM-DD). |
| `number_of_codes` | integer | **yes** | How many one-time-use codes to generate. |
| `offer_code_id` | string | **yes** | The subscriptionOfferCode ID (from create_offer_code / list_offer_codes). |
| `environment` | object | no | Optional environment (PRODUCTION or SANDBOX). |

### `create_custom_offer_code`

Create a custom (vanity) offer code for an offer-code campaign.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `custom_code` | string | **yes** | The custom (vanity) code value. |
| `number_of_codes` | integer | **yes** | Number of redemptions allowed. |
| `offer_code_id` | string | **yes** | The subscriptionOfferCode ID. |
| `expiration_date` | string | no | Optional expiration date (YYYY-MM-DD). |

### `list_offer_codes`

List the offer-code campaigns configured for a subscription.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `subscription_id` | string | **yes** | The subscription ID. |
| `limit` | integer | no | Page size (max 200). |

## Promoted purchases

Promote IAPs/subscriptions on the product page.

### `create_promoted_purchase`

Create a promoted purchase for an app, referencing either an in-app purchase or a subscription. visible_for_all_users is required.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `visible_for_all_users` | boolean | **yes** | Whether the promotion is visible to all users (required). |
| `enabled` | boolean | no | Whether the promotion is enabled. |
| `in_app_purchase_id` | string | no | The in-app purchase ID to promote (provide this OR subscription_id). |
| `subscription_id` | string | no | The subscription ID to promote (provide this OR in_app_purchase_id). |

### `update_promoted_purchase`

Update a promoted purchase (visibility and/or enabled state).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `promoted_purchase_id` | string | **yes** | The promotedPurchase ID. |
| `enabled` | boolean | no | New enabled state (optional). |
| `visible_for_all_users` | boolean | no | New visibility (optional). |

### `set_promoted_purchase_order`

Set the order of an app's promoted purchases by passing the promotedPurchase IDs in the desired order.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `ordered_promoted_purchase_ids` | array of string | **yes** | The promotedPurchase IDs in the desired display order. |

### `list_promoted_purchases`

List an app's promoted purchases (in display order).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `limit` | integer | no | Page size (max 200). |

## Customer reviews

Read reviews and post/delete developer responses.

### `list_customer_reviews`

List an app's customer reviews, optionally filtered by rating/territory and sorted (e.g. -createdDate). Set include_response=true to see existing responses.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `include_response` | boolean | no | Set to true to include each review's existing response. |
| `limit` | integer | no | Page size (max 200). |
| `rating` | integer | no | Filter by star rating (1-5). |
| `sort` | string | no | Sort, e.g. "-createdDate" (newest first) or "rating". |
| `territory` | string | no | Filter by territory, e.g. "USA". |

### `respond_to_review`

Post a developer response to a customer review. A review can have only one response; to change it, delete the existing one with delete_review_response first.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `response_body` | string | **yes** | The response body shown publicly under the review. |
| `review_id` | string | **yes** | The customerReview ID (from list_customer_reviews). |

### `delete_review_response`

Delete a developer response to a customer review by response ID.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `response_id` | string | **yes** | The customerReviewResponse ID. |

## Phased release

Staged 7-day rollout control.

### `start_phased_release`

Start a phased (7-day staged) release for an App Store version. Optionally set the initial state (defaults to ACTIVE).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `version_id` | string | **yes** | The appStoreVersion ID. |
| `state` | object | no | Optional initial state (defaults to ACTIVE). |

### `update_phased_release`

Update a phased release: PAUSED to pause, ACTIVE to resume, COMPLETE to release to all users immediately.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `phased_release_id` | string | **yes** | The appStoreVersionPhasedRelease ID. |
| `state` | `INACTIVE \| ACTIVE \| PAUSED \| COMPLETE` | **yes** | New state: PAUSED, ACTIVE, or COMPLETE. |

## Users & access

Team users and invitations.

### `list_users`

List all users on the App Store Connect team. Optionally pass include=visibleApps to include the apps each user can access.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `include` | string | no | Comma-separated related resources to include, e.g. "visibleApps". |
| `limit` | integer | no | Page size (max 200). |

### `invite_user`

Invite a new user to the App Store Connect team. Provide email, first_name, last_name, and one or more roles (e.g. DEVELOPER, ADMIN). Optionally set all_apps_visible or supply a list of visible_app_ids.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `email` | string | **yes** | Email address of the user to invite. |
| `first_name` | string | **yes** | Given name of the user. |
| `last_name` | string | **yes** | Family name of the user. |
| `roles` | array of string | **yes** | Roles to assign. Common values: ADMIN, FINANCE, DEVELOPER, MARKETING, APP_MANAGER, SALES, CUSTOMER_SUPPORT, ACCESS_TO_REPORTS, CREATE_APPS. |
| `all_apps_visible` | boolean | no | Whether the user can see all apps (true) or only the apps in visible_app_ids (false / omitted). |
| `provisioning_allowed` | boolean | no | Whether the user may access provisioning/signing resources. |
| `visible_app_ids` | array of string | no | App IDs the invited user should have access to (only used when all_apps_visible is false or omitted). |

### `update_user`

Update a team user's roles, app-visibility flag, provisioning permission, or visible apps. Only fields that are provided are sent to the API.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `user_id` | string | **yes** | The user's App Store Connect ID. |
| `all_apps_visible` | boolean | no | Whether the user should see all apps. |
| `provisioning_allowed` | boolean | no | Whether the user is allowed to provision certificates and devices. |
| `roles` | array of string | no | New roles to assign. Common values: ADMIN, FINANCE, DEVELOPER, MARKETING, APP_MANAGER, SALES, CUSTOMER_SUPPORT, ACCESS_TO_REPORTS, CREATE_APPS. |
| `visible_app_ids` | array of string | no | App IDs the user should have access to. Replaces the existing list. Only sent when provided and non-empty. |

### `remove_user`

Remove a user from the App Store Connect team by their user ID.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `user_id` | string | **yes** | The user's App Store Connect ID. |

## In-app events

App Store in-app events, localizations, and media.

### `create_app_event`

Create an in-app event for an app. Provide a reference_name (internal, not shown to customers) and optionally a badge (LIVE_EVENT, PREMIERE, CHALLENGE, COMPETITION, NEW_SEASON, MAJOR_UPDATE, SPECIAL_EVENT) and primary_locale (BCP-47, e.g. en-US). After creating, add localizations with create_app_event_localization and screenshots with upload_app_event_screenshot.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `reference_name` | string | **yes** | Internal reference name for the event (not shown to customers). |
| `badge` | object | no | Optional badge type. Common values: LIVE_EVENT, PREMIERE, CHALLENGE, COMPETITION, NEW_SEASON, MAJOR_UPDATE, SPECIAL_EVENT. |
| `primary_locale` | string | no | Optional BCP-47 primary locale for the event, e.g. "en-US". |

### `create_app_event_localization`

Add a localized name and description to an in-app event for a given locale (e.g. en-US). Provide the app_event_id, locale, name, and short_description; long_description is optional.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_event_id` | string | **yes** | The appEvent ID to attach this localization to. |
| `locale` | string | **yes** | BCP-47 locale, e.g. "en-US". |
| `name` | string | **yes** | Localized event name shown to customers. |
| `short_description` | string | **yes** | Short description shown to customers. |
| `long_description` | string | no | Optional long description shown to customers. |

### `upload_app_event_screenshot`

Upload a screenshot for an in-app event localization (reserve → upload → commit with MD5 verification). Provide the app_event_localization_id, app_event_asset_type (EVENT_CARD or EVENT_DETAILS_PAGE), and a local image file_path.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_event_asset_type` | `EVENT_CARD \| EVENT_DETAILS_PAGE` | **yes** | Asset type slot: EVENT_CARD or EVENT_DETAILS_PAGE. |
| `app_event_localization_id` | string | **yes** | The appEventLocalization ID to attach this screenshot to. |
| `file_path` | string | **yes** | Local path to the screenshot image file (PNG/JPEG). |

## Xcode Cloud

Inspect and trigger Xcode Cloud (CI) builds.

### `list_ci_products`

List all Xcode Cloud products (CI-enabled apps and frameworks) in the team. Each product corresponds to an app or framework that has been set up for Xcode Cloud.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `limit` | integer | no | Page size (max 200). |

### `list_ci_workflows`

List all CI workflows for a given Xcode Cloud product. Use list_ci_products to obtain the ci_product_id.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `ci_product_id` | string | **yes** | The Xcode Cloud product ID. |
| `limit` | integer | no | Page size (max 200). |

### `start_ci_build`

Start a new Xcode Cloud build run for a workflow on a specific branch or tag. Provide the workflow_id (from list_ci_workflows) and source_branch_or_tag_id, which is a scmGitReference resource ID. Obtain it by listing the workflow's repository git references with: appstore_list { "path": "/v1/ciWorkflows/{workflow_id}/repository/gitReferences" }.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `source_branch_or_tag_id` | string | **yes** | The scmGitReference resource ID for the branch or tag to build. Obtain it by listing the workflow's repository git references via GET /v1/ciWorkflows/{id}/repository/gitReferences (use appstore_list). |
| `workflow_id` | string | **yes** | The Xcode Cloud workflow ID. |

### `get_ci_build_run`

Get the details of a specific Xcode Cloud build run by its ID. Optionally pass include (e.g. "builds,workflows") to embed related resources.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `build_run_id` | string | **yes** | The build run ID. |
| `include` | string | no | Comma-separated related resources to include, e.g. "builds,workflows". |

### `list_ci_build_actions`

List all CI build actions (e.g. analyze, archive, test, lint) for a given Xcode Cloud build run. Use get_ci_build_run or start_ci_build to obtain the build_run_id.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `build_run_id` | string | **yes** | The build run ID. |
| `limit` | integer | no | Page size (max 200). |

## Analytics reports

Request and read App Store analytics reports.

### `request_analytics_report`

Create an analytics report request for an app. Use access_type ONGOING for a recurring report or ONE_TIME_SNAPSHOT for a one-time snapshot. Returns the report request resource including its ID, which you then pass to list_analytics_reports.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `access_type` | `ONGOING \| ONE_TIME_SNAPSHOT` | **yes** | The access type for the report request: ONGOING or ONE_TIME_SNAPSHOT. |
| `app_id` | string | **yes** | The app's App Store Connect ID. |

### `list_analytics_reports`

List the analytics reports available for a report request. Optionally filter by category (e.g. APP_USAGE, COMMERCE, ENGAGEMENT, FRAMEWORK_USAGE, PERFORMANCE). Returns report resources whose IDs you pass to list_analytics_report_instances.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `report_request_id` | string | **yes** | The analytics report request ID. |
| `category` | string | no | Filter by report category (e.g. "APP_USAGE", "COMMERCE", "ENGAGEMENT", "FRAMEWORK_USAGE", "PERFORMANCE"). |
| `limit` | integer | no | Page size (max 200). |

### `list_analytics_report_instances`

List instances of an analytics report, optionally filtered by granularity (DAILY, WEEKLY, or MONTHLY) and/or processing date (YYYY-MM-DD). Returns instance resources whose IDs you pass to list_analytics_report_segments.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `report_id` | string | **yes** | The analytics report ID. |
| `granularity` | object | no | Filter by granularity: DAILY, WEEKLY, or MONTHLY. |
| `limit` | integer | no | Page size (max 200). |
| `processing_date` | string | no | Filter by processing date in YYYY-MM-DD format. |

### `list_analytics_report_segments`

List the downloadable segments for an analytics report instance. Each segment's attributes include a presigned `url` pointing to a gzipped CSV file, plus `sizeInBytes` and `checksum` — this tool surfaces those download URLs rather than downloading the data itself.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `instance_id` | string | **yes** | The analytics report instance ID. |
| `limit` | integer | no | Page size (max 200). |

## Custom product pages

Marketing product-page variants: pages, versions, localized text, and image sets.

### `list_custom_product_pages`

List an app's custom product pages (CPP).

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `include` | string | no | Comma-separated includes, e.g. "appCustomProductPageVersions". |
| `limit` | integer | no | Page size (max 200). |

### `get_custom_product_page`

Get a custom product page by ID, with optional includes.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `page_id` | string | **yes** | The appCustomProductPage ID. |
| `include` | string | no | Comma-separated includes, e.g. "appCustomProductPageVersions". |

### `create_custom_product_page`

Create a custom product page for an app (reference name). Then add a version, localizations, and screenshot/preview sets.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `app_id` | string | **yes** | The app's App Store Connect ID. |
| `name` | string | **yes** | Reference name for the page (not customer-facing). |

### `update_custom_product_page`

Update a custom product page's name and/or visibility.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `page_id` | string | **yes** | The appCustomProductPage ID. |
| `name` | string | no | New reference name. |
| `visible` | boolean | no | Whether the page is visible/active. |

### `delete_custom_product_page`

Delete a custom product page by ID.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `page_id` | string | **yes** | The appCustomProductPage ID. |

### `list_custom_product_page_versions`

List a custom product page's versions.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `page_id` | string | **yes** | The appCustomProductPage ID. |
| `limit` | integer | no | Page size (max 200). |

### `create_custom_product_page_version`

Create a new version of a custom product page (optionally with a deep link). A new version is the editable draft you add localizations and images to.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `page_id` | string | **yes** | The appCustomProductPage ID. |
| `deep_link` | string | no | Optional deep link URL the page opens to in the app. |

### `list_custom_product_page_localizations`

List a custom product page version's localizations.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `version_id` | string | **yes** | The appCustomProductPageVersion ID. |
| `limit` | integer | no | Page size (max 200). |

### `create_custom_product_page_localization`

Add a localized promotional text to a custom product page version for a given locale. Create screenshot/preview sets against the returned localization ID.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `locale` | string | **yes** | BCP-47 locale, e.g. "en-US". |
| `version_id` | string | **yes** | The appCustomProductPageVersion ID. |
| `promotional_text` | string | no | Optional promotional text shown on the page for this locale. |

### `update_custom_product_page_localization`

Update a custom product page localization's promotional text.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `localization_id` | string | **yes** | The appCustomProductPageLocalization ID. |
| `promotional_text` | string | **yes** | New promotional text. |

### `create_cpp_screenshot_set`

Create an appScreenshotSet on a custom product page localization (e.g. display type APP_IPHONE_67). Upload images into it with upload_app_screenshot.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `localization_id` | string | **yes** | The appCustomProductPageLocalization ID. |
| `screenshot_display_type` | string | **yes** | Display type, e.g. "APP_IPHONE_67", "APP_IPAD_PRO_129". |

### `create_cpp_preview_set`

Create an appPreviewSet on a custom product page localization (e.g. preview type IPHONE_67). Upload videos into it with upload_app_preview.

| Parameter | Type | Required | Description |
|---|---|---|---|
| `localization_id` | string | **yes** | The appCustomProductPageLocalization ID. |
| `preview_type` | string | **yes** | Preview type, e.g. "IPHONE_67", "IPAD_PRO_129". |

