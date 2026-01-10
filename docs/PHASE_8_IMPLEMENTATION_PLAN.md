# Phase 8 Implementation Plan: Production & Distribution

## Overview

Phase 8 transforms Midlight from a development build into a production-ready, distributable application. This phase covers auto-updates, native platform integration, code signing, and build automation.

**Important:** This plan leverages the existing release infrastructure from the Electron app (ai-doc-app) to maintain consistency and reduce operational overhead.

---

## Implementation Status (Audit: January 2026)

| Component | Status | Completion |
|-----------|--------|------------|
| **Auto-Update System** | ✅ Complete | Rust commands, store, client, UpdateDialog UI all implemented |
| **Native Menu (macOS)** | ✅ Complete | Full menu.rs with App/File/Edit/View/Window/Help menus |
| **Code Signing Config** | ✅ Complete | release.yml has Apple + Azure Trusted Signing configured |
| **Build Pipeline** | ✅ Complete | check.yml + release.yml with all 3 platforms |
| **System Operations** | ✅ Complete | show_in_folder, open_external, window state persistence |
| **Secrets & Distribution** | ✅ Complete | All secrets added, successful build completed |
| **DOCX Import** | ✅ Complete | Full implementation with zip+quick-xml parser, Tiptap conversion |

**✅ Phase 8 Core Implementation COMPLETE (January 2026)**

All signing secrets have been added to GitHub and a successful release build has been completed.
DOCX import feature fully implemented with dedicated DocxImportDialog UI.

**Polish Items (Completed January 2026):**
- [x] Clean up Azure debug code in release.yml
- [x] Theme detection integration (listens for system theme changes)
- [x] Tray icon implementation (system tray with Show/Quit menu)
- [ ] Final cross-platform testing (manual verification)

---

## Existing Infrastructure (from ai-doc-app)

Before diving into implementation, here's what we already have:

| Component | Status | Details |
|-----------|--------|---------|
| **Update Server** | Active | `https://midlight.ai/releases/` via Caddy static files |
| **Production Server** | Active | Digital Ocean droplet with PM2, Caddy |
| **Apple Developer** | Active | Team ID: `M9KYJP7UP3`, notarization configured |
| **Windows Signing** | Active | Azure Trusted Signing service |
| **GitHub Actions** | Active | Build workflow with parallel macOS/Windows builds |
| **Build Assets** | Available | Icons, entitlements in `ai-doc-app/build/` |
| **Deploy Pipeline** | Active | SCP to `/var/www/midlight-releases/` |

---

## Table of Contents

1. [Auto-Update System](#1-auto-update-system)
2. [Native Menu Integration](#2-native-menu-integration)
3. [Code Signing & Notarization](#3-code-signing--notarization)
4. [Build Pipeline & CI/CD](#4-build-pipeline--cicd)
5. [System Operations](#5-system-operations)
6. [DOCX Import](#6-docx-import)
7. [Implementation Order](#7-implementation-order)
8. [Progress Tracking](#8-progress-tracking)

---

## 1. Auto-Update System

### Current State ✅ IMPLEMENTED
- `tauri-plugin-updater` v2 installed and initialized in `lib.rs`
- `tauri.conf.json` configured with pubkey and endpoint (`midlight.ai/releases/tauri-latest.json`)
- **Rust commands:** `check_for_updates`, `download_and_install_update`, `get_current_version`
- **Svelte store:** Full state management with derived stores for UI
- **Frontend client:** Automatic periodic checks (10s initial, 4hr interval)
- **UpdateDialog.svelte:** Complete UI with progress bar, states, restart button
- **App.svelte integration:** Client initialized on mount, destroyed on unmount
- **Existing Electron app uses `midlight.ai/releases/` for updates (Tauri uses same server, different manifest)**

### Options Analysis

#### Option A: GitHub Releases
**Description:** Use GitHub Releases as the update server with Tauri's built-in support.

**Pros:**
- Zero infrastructure cost
- Native Tauri integration
- CDN-backed delivery

**Cons:**
- **Requires separate release infrastructure from Electron app**
- Less control over update analytics
- Rate limits on API calls

#### Option B: Self-Hosted at midlight.ai/releases/ (Recommended)
**Description:** Use existing Digital Ocean infrastructure that serves Electron updates.

**Pros:**
- **Consistency with existing Electron release pipeline**
- **Infrastructure already operational and tested**
- **Same deployment process for both apps**
- Full control over update logic
- Works with private repos
- Can serve both Electron and Tauri apps side-by-side

**Cons:**
- Need to generate Tauri-specific update manifest (JSON vs YAML)
- Must adapt workflow for Tauri's update format

#### Option C: S3/CloudFlare R2 + Static Manifest
**Description:** Host binaries on object storage with static JSON manifest.

**Pros:**
- Low cost at scale
- Global CDN distribution

**Cons:**
- **Introduces third infrastructure, increases complexity**
- Manual manifest updates (unless automated)

### Recommendation: Option B (Self-Hosted at midlight.ai/releases/)

Using the existing infrastructure provides consistency between Electron and Tauri releases. Both apps can coexist in the same `/var/www/midlight-releases/` directory with different file naming conventions. The Caddy configuration already handles static file serving with proper caching.

### Tauri vs Electron Update Format

| Aspect | Electron (electron-updater) | Tauri (tauri-plugin-updater) |
|--------|----------------------------|------------------------------|
| Manifest | `latest-mac.yml`, `latest-win.yml` | `latest.json` (single file) |
| Signature | SHA512 checksums | minisign signatures |
| Format | YAML | JSON |

**File naming convention for coexistence:**
```
/var/www/midlight-releases/
├── Midlight-0.0.54-arm64.dmg          # Electron macOS
├── Midlight-0.0.54.exe                # Electron Windows
├── latest-mac.yml                      # Electron update manifest
├── latest-win.yml                      # Electron update manifest
├── midlight-next-0.1.0-macos.dmg      # Tauri macOS (different prefix)
├── midlight-next-0.1.0-windows.msi    # Tauri Windows
├── midlight-next-0.1.0-linux.AppImage # Tauri Linux
└── tauri-latest.json                   # Tauri update manifest
```

### Implementation Tasks

#### 1.1 Generate Update Keys
```bash
# Generate keypair for update signing (store private key securely!)
cargo tauri signer generate -w ~/.tauri/midlight-next.key
# Output: Public key to add to tauri.conf.json
# Store TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD in GitHub secrets
```

#### 1.2 Configure tauri.conf.json
```json
{
  "plugins": {
    "updater": {
      "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk...",
      "endpoints": [
        "https://midlight.ai/releases/tauri-latest.json"
      ],
      "windows": {
        "installMode": "passive"
      }
    }
  }
}
```

#### 1.3 Tauri Update Manifest Format
The `tauri-latest.json` file generated during build:
```json
{
  "version": "0.1.0",
  "notes": "Bug fixes and performance improvements",
  "pub_date": "2024-01-15T12:00:00Z",
  "platforms": {
    "darwin-aarch64": {
      "signature": "dW50cnVzdGVkIGNvbW1lbnQ6...",
      "url": "https://midlight.ai/releases/midlight-next-0.1.0-macos-aarch64.dmg"
    },
    "darwin-x86_64": {
      "signature": "dW50cnVzdGVkIGNvbW1lbnQ6...",
      "url": "https://midlight.ai/releases/midlight-next-0.1.0-macos-x86_64.dmg"
    },
    "windows-x86_64": {
      "signature": "dW50cnVzdGVkIGNvbW1lbnQ6...",
      "url": "https://midlight.ai/releases/midlight-next-0.1.0-windows.msi"
    },
    "linux-x86_64": {
      "signature": "dW50cnVzdGVkIGNvbW1lbnQ6...",
      "url": "https://midlight.ai/releases/midlight-next-0.1.0-linux.AppImage"
    }
  }
}
```

#### 1.3 Create Update Commands (Rust)
```rust
// src-tauri/src/commands/updates.rs
#[tauri::command]
pub async fn check_for_updates(app: tauri::AppHandle) -> Result<Option<UpdateInfo>, String>;

#[tauri::command]
pub async fn download_and_install_update(app: tauri::AppHandle) -> Result<(), String>;

#[tauri::command]
pub fn get_current_version() -> String;
```

#### 1.4 Create Update UI (Svelte)
- `UpdateDialog.svelte` - Modal showing update availability
- `UpdateProgress.svelte` - Download progress indicator
- Integration with app startup check

#### 1.5 Create Frontend Store
```typescript
// packages/stores/src/updates.ts
interface UpdateState {
  checking: boolean;
  available: UpdateInfo | null;
  downloading: boolean;
  progress: number;
  error: string | null;
}
```

---

## 2. Native Menu Integration

### Current State ✅ IMPLEMENTED
- **macOS:** Native menu bar via `menu.rs` with 6 menus (App, File, Edit, View, Window, Help)
- **Windows/Linux:** Custom `WindowsMenu.svelte` menu (existing)
- **menu.rs:** 16 custom menu items with keyboard shortcuts, emits events to frontend
- **App.svelte:** Event listeners for all 14 custom menu actions
- Platform detection in TitleBar.svelte renders WindowsMenu only on non-macOS

### Options Analysis

#### Option A: Hybrid Approach (Recommended)
**Description:** Native menu bar on macOS, custom HTML menu on Windows/Linux.

**Pros:**
- Best UX on each platform
- macOS users get expected behavior (Cmd+Q, app menu, etc.)
- Windows/Linux users keep current working menu
- Leverages existing WindowsMenu.svelte

**Cons:**
- Two code paths to maintain
- Need to sync menu items between Rust and Svelte

**Implementation:**
- Rust: Build native menu for macOS using Tauri's Menu API
- Svelte: Conditionally render WindowsMenu only on non-macOS

#### Option B: Fully Native Menus (All Platforms)
**Description:** Use Tauri's Menu API for all platforms.

**Pros:**
- Single source of truth for menu structure
- Consistent behavior across platforms
- Better OS integration (recent files, etc.)

**Cons:**
- Less styling flexibility
- Windows native menus look dated
- Significant rewrite of current menu logic

#### Option C: Custom Menus Everywhere
**Description:** Keep current HTML menu approach on all platforms.

**Pros:**
- Full styling control
- Single codebase
- Already working

**Cons:**
- Non-standard macOS experience (dealbreaker for many users)
- Accessibility concerns
- No global keyboard shortcuts

### Recommendation: Option A (Hybrid)

macOS users have strong expectations about native menus. The current custom menu works well on Windows/Linux where users are accustomed to varied menu styles. The hybrid approach provides optimal UX with minimal additional maintenance.

### Implementation Tasks

#### 2.1 Create Menu Module (Rust)
```rust
// src-tauri/src/menu.rs
pub fn create_macos_menu(app: &App) -> Menu<Wry> {
    let app_menu = SubmenuBuilder::new(app, "Midlight")
        .about(None)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;

    let file_menu = SubmenuBuilder::new(app, "File")
        .item(&MenuItem::with_id(app, "new_file", "New File", true, Some("CmdOrCtrl+N"))?)
        .item(&MenuItem::with_id(app, "new_folder", "New Folder", true, Some("CmdOrCtrl+Shift+N"))?)
        .separator()
        .item(&MenuItem::with_id(app, "save", "Save", true, Some("CmdOrCtrl+S"))?)
        // ... more items
        .build()?;

    // Edit, View, Window, Help menus...

    MenuBuilder::new(app)
        .items(&[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu, &help_menu])
        .build()
}
```

#### 2.2 Register Menu in lib.rs
```rust
.setup(|app| {
    #[cfg(target_os = "macos")]
    {
        let menu = menu::create_macos_menu(app)?;
        app.set_menu(menu)?;
    }
    Ok(())
})
```

#### 2.3 Handle Menu Events
```rust
.on_menu_event(|app, event| {
    match event.id().as_ref() {
        "new_file" => { /* emit to frontend */ }
        "save" => { /* emit to frontend */ }
        "quit" => { app.exit(0); }
        _ => {}
    }
})
```

#### 2.4 Update WindowsMenu.svelte
```svelte
<script>
  import { platform } from '@tauri-apps/plugin-os';

  const isMacOS = platform() === 'macos';
</script>

{#if !isMacOS}
  <!-- existing menu markup -->
{/if}
```

---

## 3. Code Signing & Notarization

### Current State ✅ CONFIGURED (Pending Secrets)
- **Apple Developer account:** Active (Team ID: `M9KYJP7UP3`)
- **Azure Trusted Signing:** Configured in release.yml using `azure/trusted-signing-action@v0.5.0`
- **release.yml:** Full macOS signing with certificate import, keychain creation, notarization
- **release.yml:** Windows MSI signing via Azure Trusted Signing
- **entitlements.mac.plist:** Copied from Electron app
- **Icons:** All present in `src-tauri/icons/` (icns, ico, png files)
- **Pending:** Add secrets to GitHub repo (manual step)

### Existing Credentials (from ai-doc-app)

**macOS (Apple Developer):**
- Team ID: `M9KYJP7UP3`
- Signing Identity: `Developer ID Application: ...`
- Notarization configured with app-specific password
- Entitlements file: `ai-doc-app/build/entitlements.mac.plist`

**Windows (Azure Trusted Signing):**
- Service already operational (no hardware token needed!)
- Credentials in GitHub secrets:
  - `AZURE_TENANT_ID`
  - `AZURE_CLIENT_ID`
  - `AZURE_CLIENT_SECRET`
  - `AZURE_ENDPOINT`
  - `AZURE_CODE_SIGNING_ACCOUNT`
  - `AZURE_CERT_PROFILE_NAME`

### Recommendation: Use Existing Infrastructure

No new certificates needed! We already have:
1. Apple Developer with notarization working
2. Azure Trusted Signing (cloud-based, no hardware token)

This is better than OV certificates because Azure Trusted Signing provides immediate SmartScreen trust without the hardware token complexity of traditional EV certificates.

### Implementation Tasks

#### 3.1 Copy Build Assets from Electron App
```bash
# Copy icons and entitlements
cp -r ../ai-doc-app/build/icon.icns apps/desktop/src-tauri/icons/
cp -r ../ai-doc-app/build/icon.ico apps/desktop/src-tauri/icons/
cp -r ../ai-doc-app/build/icon.png apps/desktop/src-tauri/icons/
cp ../ai-doc-app/build/entitlements.mac.plist apps/desktop/src-tauri/
```

#### 3.2 Configure macOS Signing (tauri.conf.json)
```json
{
  "bundle": {
    "macOS": {
      "signingIdentity": "-",
      "entitlements": "entitlements.mac.plist",
      "minimumSystemVersion": "10.13"
    }
  }
}
```

Note: Tauri uses environment variables for signing, not config file:
```bash
# Set in GitHub Actions (same secrets as Electron workflow)
APPLE_SIGNING_IDENTITY="Developer ID Application: Scott Daly (M9KYJP7UP3)"
APPLE_ID="<from-secrets>"
APPLE_PASSWORD="<app-specific-password>"
APPLE_TEAM_ID="M9KYJP7UP3"
```

#### 3.3 Configure Windows Signing with Azure Trusted Signing

Tauri supports Azure Trusted Signing via the `tauri-plugin-cli`:

```json
// tauri.conf.json
{
  "bundle": {
    "windows": {
      "signCommand": "AzureSignTool sign -kvu $AZURE_ENDPOINT -kvi $AZURE_CLIENT_ID -kvt $AZURE_TENANT_ID -kvs $AZURE_CLIENT_SECRET -kvc $AZURE_CERT_PROFILE_NAME -tr http://timestamp.acs.microsoft.com -td sha256 \"%1\""
    }
  }
}
```

Or use a custom signing script in the workflow (matching Electron approach):
```yaml
- name: Sign Windows executable
  if: matrix.platform == 'windows-latest'
  run: |
    AzureSignTool sign \
      -kvu "${{ secrets.AZURE_ENDPOINT }}" \
      -kvi "${{ secrets.AZURE_CLIENT_ID }}" \
      -kvt "${{ secrets.AZURE_TENANT_ID }}" \
      -kvs "${{ secrets.AZURE_CLIENT_SECRET }}" \
      -kvc "${{ secrets.AZURE_CERT_PROFILE_NAME }}" \
      -tr http://timestamp.acs.microsoft.com \
      -td sha256 \
      "target/release/bundle/msi/*.msi"
```

#### 3.4 Entitlements File
Copy from Electron app or create:
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.cs.allow-jit</key>
    <true/>
    <key>com.apple.security.cs.allow-unsigned-executable-memory</key>
    <true/>
    <key>com.apple.security.cs.disable-library-validation</key>
    <true/>
    <key>com.apple.security.automation.apple-events</key>
    <true/>
</dict>
</plist>
```

---

## 4. Build Pipeline & CI/CD

### Current State ✅ IMPLEMENTED
- **check.yml:** PR validation (TypeScript build/lint, Cargo check/clippy/fmt)
- **release.yml:** Full release workflow with:
  - Tag-triggered (`v*`) or manual dispatch
  - Parallel macOS/Windows/Linux builds
  - macOS: Universal binary, code signing, notarization
  - Windows: MSI with Azure Trusted Signing
  - Linux: AppImage + deb
  - Deploy phase: SCP to `/var/www/midlight-releases/`
  - `tauri-latest.json` manifest generation
- **version.sh:** Cross-platform version bump script
- **Electron app workflow** available for reference (`ai-doc-app/.github/workflows/build.yml`)

### Existing Electron Workflow Pattern

The Electron workflow has these key features we should replicate:
1. Tag-triggered releases (`v*`)
2. Parallel macOS + Windows builds
3. Code signing with Apple notarization + Azure Trusted Signing
4. Deploy phase that SCPs to `/var/www/midlight-releases/`
5. `version.json` generation for download page

### Recommendation: Mirror Electron Workflow

Copy the proven pattern from the Electron app, adapted for Tauri:
- Same deployment target (SCP to Digital Ocean)
- Same signing infrastructure
- Same artifact naming convention (with `midlight-next-` prefix)

### Implementation Tasks

#### 4.1 Create Release Workflow (Mirrors Electron)
```yaml
# .github/workflows/release.yml
name: Release Tauri App

on:
  push:
    tags:
      - 'v*'
  workflow_dispatch:

jobs:
  build-macos:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Node
        uses: actions/setup-node@v4
        with:
          node-version: 20

      - name: Setup pnpm
        uses: pnpm/action-setup@v4
        with:
          version: 9

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-apple-darwin,x86_64-apple-darwin

      - name: Install dependencies
        run: pnpm install

      - name: Build Tauri App (Universal)
        run: pnpm tauri build --target universal-apple-darwin
        working-directory: apps/desktop
        env:
          APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
          APPLE_ID: ${{ secrets.APPLE_ID }}
          APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
          APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}

      - name: Upload macOS artifacts
        uses: actions/upload-artifact@v4
        with:
          name: macos-release
          path: |
            apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/dmg/*.dmg
            apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/macos/*.app.tar.gz
            apps/desktop/src-tauri/target/universal-apple-darwin/release/bundle/macos/*.app.tar.gz.sig

  build-windows:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Node
        uses: actions/setup-node@v4
        with:
          node-version: 20

      - name: Setup pnpm
        uses: pnpm/action-setup@v4
        with:
          version: 9

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install dependencies
        run: pnpm install

      - name: Build Tauri App
        run: pnpm tauri build
        working-directory: apps/desktop
        env:
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}

      - name: Install AzureSignTool
        run: dotnet tool install --global AzureSignTool

      - name: Sign Windows MSI
        run: |
          AzureSignTool sign `
            -kvu "${{ secrets.AZURE_ENDPOINT }}" `
            -kvi "${{ secrets.AZURE_CLIENT_ID }}" `
            -kvt "${{ secrets.AZURE_TENANT_ID }}" `
            -kvs "${{ secrets.AZURE_CLIENT_SECRET }}" `
            -kvc "${{ secrets.AZURE_CERT_PROFILE_NAME }}" `
            -tr http://timestamp.acs.microsoft.com `
            -td sha256 `
            apps/desktop/src-tauri/target/release/bundle/msi/*.msi

      - name: Upload Windows artifacts
        uses: actions/upload-artifact@v4
        with:
          name: windows-release
          path: |
            apps/desktop/src-tauri/target/release/bundle/msi/*.msi
            apps/desktop/src-tauri/target/release/bundle/msi/*.msi.sig

  build-linux:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4

      - name: Install system dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf

      - name: Setup Node
        uses: actions/setup-node@v4
        with:
          node-version: 20

      - name: Setup pnpm
        uses: pnpm/action-setup@v4
        with:
          version: 9

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install dependencies
        run: pnpm install

      - name: Build Tauri App
        run: pnpm tauri build
        working-directory: apps/desktop
        env:
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}

      - name: Upload Linux artifacts
        uses: actions/upload-artifact@v4
        with:
          name: linux-release
          path: |
            apps/desktop/src-tauri/target/release/bundle/appimage/*.AppImage
            apps/desktop/src-tauri/target/release/bundle/appimage/*.AppImage.sig
            apps/desktop/src-tauri/target/release/bundle/deb/*.deb

  deploy:
    needs: [build-macos, build-windows, build-linux]
    runs-on: ubuntu-latest
    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts

      - name: Flatten artifacts
        run: |
          mkdir -p deploy
          find artifacts -type f \( -name "*.dmg" -o -name "*.msi" -o -name "*.AppImage" -o -name "*.deb" -o -name "*.sig" -o -name "*.tar.gz" \) -exec cp {} deploy/ \;

      - name: Generate tauri-latest.json
        run: |
          VERSION=${GITHUB_REF_NAME#v}
          cat > deploy/tauri-latest.json << EOF
          {
            "version": "$VERSION",
            "notes": "See release notes at https://midlight.ai/changelog",
            "pub_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
            "platforms": {
              "darwin-universal": {
                "signature": "$(cat deploy/*.app.tar.gz.sig 2>/dev/null || echo '')",
                "url": "https://midlight.ai/releases/midlight-next-$VERSION-macos.app.tar.gz"
              },
              "windows-x86_64": {
                "signature": "$(cat deploy/*.msi.sig 2>/dev/null || echo '')",
                "url": "https://midlight.ai/releases/midlight-next-$VERSION-windows.msi"
              },
              "linux-x86_64": {
                "signature": "$(cat deploy/*.AppImage.sig 2>/dev/null || echo '')",
                "url": "https://midlight.ai/releases/midlight-next-$VERSION-linux.AppImage"
              }
            }
          }
          EOF

      - name: Rename artifacts with version
        run: |
          VERSION=${GITHUB_REF_NAME#v}
          cd deploy
          for f in *.dmg; do [ -f "$f" ] && mv "$f" "midlight-next-$VERSION-macos.dmg"; done
          for f in *.msi; do [ -f "$f" ] && mv "$f" "midlight-next-$VERSION-windows.msi"; done
          for f in *.AppImage; do [ -f "$f" ] && mv "$f" "midlight-next-$VERSION-linux.AppImage"; done

      - name: Setup SSH
        run: |
          mkdir -p ~/.ssh
          echo "${{ secrets.DEPLOY_KEY }}" > ~/.ssh/deploy_key
          chmod 600 ~/.ssh/deploy_key
          ssh-keyscan -H ${{ secrets.DEPLOY_HOST }} >> ~/.ssh/known_hosts

      - name: Deploy to server
        run: |
          scp -i ~/.ssh/deploy_key deploy/* ${{ secrets.DEPLOY_USER }}@${{ secrets.DEPLOY_HOST }}:/var/www/midlight-releases/

#### 4.2 Create PR Check Workflow
```yaml
# .github/workflows/check.yml
name: Check

on:
  pull_request:
  push:
    branches: [main]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: 'pnpm'
      - run: pnpm install
      - run: pnpm build
      - run: pnpm test
      - run: pnpm lint

  rust-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install system dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - run: cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml
      - run: cargo clippy --manifest-path apps/desktop/src-tauri/Cargo.toml -- -D warnings
      - run: cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml
```

#### 4.3 Version Management
Sync version across all config files:
```bash
#!/bin/bash
# scripts/version.sh
VERSION=$1
# Root package.json
sed -i '' "s/\"version\": \".*\"/\"version\": \"$VERSION\"/" package.json
# Desktop app package.json
sed -i '' "s/\"version\": \".*\"/\"version\": \"$VERSION\"/" apps/desktop/package.json
# Cargo.toml
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" apps/desktop/src-tauri/Cargo.toml
# tauri.conf.json
sed -i '' "s/\"version\": \".*\"/\"version\": \"$VERSION\"/" apps/desktop/src-tauri/tauri.conf.json
echo "Updated version to $VERSION"
```

#### 4.4 Required GitHub Secrets

Reuse from Electron workflow (already configured):
```
# Apple (macOS signing + notarization)
APPLE_SIGNING_IDENTITY
APPLE_ID
APPLE_PASSWORD
APPLE_TEAM_ID
CSC_LINK (certificate)
CSC_KEY_PASSWORD

# Azure Trusted Signing (Windows)
AZURE_TENANT_ID
AZURE_CLIENT_ID
AZURE_CLIENT_SECRET
AZURE_ENDPOINT
AZURE_CODE_SIGNING_ACCOUNT
AZURE_CERT_PROFILE_NAME

# Tauri Update Signing
TAURI_SIGNING_PRIVATE_KEY
TAURI_SIGNING_PRIVATE_KEY_PASSWORD

# Deployment
DEPLOY_KEY
DEPLOY_HOST
DEPLOY_USER
```

---

## 5. System Operations

### Current State ✅ IMPLEMENTED
- **Rust commands:** `show_in_folder`, `open_external`, `get_app_version`, `get_platform_info`
- **Frontend client:** `apps/desktop/src/lib/system.ts` wrapping all commands
- **Window state persistence:** Full implementation with debounced saves
  - Store: `packages/stores/src/windowState.ts`
  - Client: `apps/desktop/src/lib/windowState.ts` (uses tauri-plugin-store)
  - Persists: position, size, maximized, fullscreen states
  - Smart handling: doesn't save position when maximized/fullscreen
- **App.svelte integration:** Window state client initialized on mount
- **Optional (not implemented):** Tray icon, theme detection integration

### Implementation Tasks

#### 5.1 Show in Finder/Explorer
```rust
// src-tauri/src/commands/system.rs
#[tauri::command]
pub fn show_in_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .args(["/select,", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(Path::new(&path).parent().unwrap_or(Path::new(&path)))
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
```

#### 5.2 Open External URLs
```rust
#[tauri::command]
pub fn open_external(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| e.to_string())
}
```

#### 5.3 System Theme Detection
```rust
// Already have tauri-plugin-os
// Frontend can use:
import { theme } from '@tauri-apps/plugin-os';
const currentTheme = await theme(); // 'light' | 'dark'
```

#### 5.4 Window State Persistence
```typescript
// packages/stores/src/windowState.ts
interface WindowState {
  width: number;
  height: number;
  x: number;
  y: number;
  maximized: boolean;
}
// Save on window close, restore on app start
```

#### 5.5 Tray Icon (Optional, Lower Priority)
```rust
// Only if users request it
use tauri::tray::{TrayIconBuilder, TrayIconEvent};

TrayIconBuilder::new()
    .icon(app.default_window_icon().unwrap().clone())
    .menu(&tray_menu)
    .on_tray_icon_event(|tray, event| {
        // Handle click events
    })
    .build(app)?;
```

---

## 6. DOCX Import

### Current State ✅ IMPLEMENTED (January 2026)

**Full implementation complete.** See [DOCX_IMPORT_IMPLEMENTATION_PLAN.md](./DOCX_IMPORT_IMPLEMENTATION_PLAN.md) for details.

| Component | Status | Location |
|-----------|--------|----------|
| Core Parser | ✅ Complete | `src/services/docx_import.rs` (~850 lines) |
| Tiptap Conversion | ✅ Complete | Paragraphs, headings, lists, formatting, images |
| Image Extraction | ✅ Complete | From word/media/ with relationship ID mapping |
| Tauri Commands | ✅ Complete | `import_select_docx_file`, `import_analyze_docx`, `import_docx_file` |
| Frontend Client | ✅ Complete | `apps/desktop/src/lib/import.ts` |
| UI Dialog | ✅ Complete | `DocxImportDialog.svelte` (dedicated 4-step wizard) |
| Menu Integration | ✅ Complete | File > Import Word Document... |

### Implementation Approach (Actual)

Used **zip + quick-xml** crates instead of docx-rs for reading:
- docx-rs is primarily a writing library with limited read support
- Direct XML parsing gives full control over Word's DocumentML format
- Extracts images from word/media/ using relationship IDs from word/_rels/document.xml.rels

### Supported Elements

| Element | Support |
|---------|---------|
| Paragraphs | ✅ Full |
| Headings (H1-H6) | ✅ Via Word styles |
| Bold, Italic, Underline, Strike | ✅ Full |
| Text color | ✅ Hex colors |
| Highlight | ✅ Word highlight colors mapped |
| Font size & family | ✅ Full |
| Bullet lists | ✅ Full |
| Numbered lists | ✅ Full |
| Nested lists | ✅ Via numPr level |
| Images | ✅ Extracted and embedded |
| Tables | ⚠️ Converted to text (warning shown) |

---

## 7. Implementation Order

### Phase 8.1: Foundation ✅ COMPLETE
1. ✅ Generate update signing keys (`~/.tauri/midlight-next.key`)
2. ✅ Configure tauri.conf.json for updates (pubkey, endpoint)
3. ✅ Set up GitHub Actions check workflow (`.github/workflows/check.yml`)
4. ✅ Create system commands (show_in_folder, open_external, get_app_version, get_platform_info)

### Phase 8.2: Auto-Updates ✅ COMPLETE
5. ✅ Create update commands (Rust) - `commands/updates.rs`
6. ✅ Create update store and client (TypeScript) - `packages/stores/src/updates.ts`, `apps/desktop/src/lib/updates.ts`
7. ✅ Build UpdateDialog.svelte
8. ⬜ Test update flow locally (pending: need to run local update server)

### Phase 8.3: Native Menus ✅ COMPLETE
9. ✅ Create menu.rs with macOS menu (App, File, Edit, View, Window, Help)
10. ✅ Register menu in lib.rs setup (macOS only)
11. ✅ Handle menu events (App.svelte listeners)
12. ✅ Platform detection (TitleBar.svelte already had this)

### Phase 8.4: Signing & Distribution ✅ COMPLETE
13. ✅ Entitlements copied from Electron app
14. ✅ Release workflow created (`.github/workflows/release.yml`)
15. ✅ Icons already present in `src-tauri/icons/` (icns, ico, png files)
16. ✅ macOS signing configured in release.yml (Apple cert, notarization)
17. ✅ Windows Azure Trusted Signing configured in release.yml
18. ✅ Linux build with AppImage + deb
19. ✅ Secrets added to GitHub repo
20. ✅ Successful build completed (January 2026)

### Phase 8.5: Polish ✅ COMPLETE
21. ✅ Window state persistence (store + client + App.svelte integration)
22. ✅ Theme detection integration (listens for system theme changes in real-time)
23. ✅ DOCX import (complete - see DOCX_IMPORT_IMPLEMENTATION_PLAN.md)
24. ✅ Tray icon implementation (system tray with Show/Quit menu)
25. ✅ Clean up Azure debug code in release.yml
26. ⬜ Final testing across platforms (manual verification)

---

## 8. Progress Tracking

### 8.1 Auto-Update System (Self-Hosted at midlight.ai/releases/)
- [x] Generate Tauri update signing keypair (`~/.tauri/midlight-next.key`)
- [ ] Store private key in GitHub secrets (manual step required)
- [x] Configure updater in tauri.conf.json (endpoint: `midlight.ai/releases/tauri-latest.json`)
- [x] Create `commands/updates.rs`
  - [x] `check_for_updates` command
  - [x] `download_and_install_update` command
  - [x] `get_current_version` command
- [x] Register commands in lib.rs
- [x] Create `packages/stores/src/updates.ts`
- [x] Create `apps/desktop/src/lib/updates.ts` (updates client)
- [x] Create `UpdateDialog.svelte`
- [x] Integrate update check on app startup (10s delay, 4hr periodic checks)
- [ ] Test update flow end-to-end with local server

### 8.2 Native Menu Integration
- [x] Create `src-tauri/src/menu.rs`
  - [x] App menu (About, Check for Updates, Settings, Quit)
  - [x] File menu (New Document, Open Workspace, Save, Export DOCX/PDF, Close Tab)
  - [x] Edit menu (Undo, Redo, Cut, Copy, Paste, Select All, Find)
  - [x] View menu (Toggle AI Panel, Toggle Versions Panel, Fullscreen)
  - [x] Window menu (Minimize, Maximize, Close)
  - [x] Help menu (Documentation, Report Issue)
- [x] Register menu in lib.rs setup (macOS only)
- [x] Handle menu events with frontend emission (App.svelte listeners)
- [x] Platform detection in TitleBar.svelte (WindowsMenu only on non-Mac)
- [ ] Test menu behavior on macOS
- [ ] Verify Windows/Linux menu still works

### 8.3 Code Signing & Notarization (Leverage Existing)
- [x] Icons present in `apps/desktop/src-tauri/icons/` (icns, ico, png files)
- [x] Copy entitlements.mac.plist from Electron app
- [x] Configure macOS signing in release.yml (certificate import, signing identity, notarization)
- [x] Configure Azure Trusted Signing for Windows MSI in release.yml
- [x] Add secrets to GitHub repo
- [x] Successful build completed (January 2026)

### 8.4 Build Pipeline & CI/CD (Mirror Electron)
- [x] Create `.github/workflows/check.yml` (PR checks)
  - [x] TypeScript build + lint
  - [x] Cargo check + clippy
  - [ ] Tests (when test infrastructure is set up)
- [x] Create `.github/workflows/release.yml`
  - [x] macOS universal build with signing
  - [x] Windows x64 build with Azure signing
  - [x] Linux x64 build (AppImage + deb)
  - [x] Deploy phase (SCP to `/var/www/midlight-releases/`)
  - [x] `tauri-latest.json` manifest generation
- [x] Add secrets to GitHub repo
  - [x] TAURI_SIGNING_PRIVATE_KEY
  - [x] TAURI_SIGNING_PRIVATE_KEY_PASSWORD
  - [x] Apple signing secrets (APPLE_CERTIFICATE, APPLE_CERTIFICATE_PASSWORD, APPLE_ID, APPLE_PASSWORD, APPLE_TEAM_ID)
  - [x] Azure secrets (AZURE_TENANT_ID, AZURE_CLIENT_ID, AZURE_CLIENT_SECRET, AZURE_ENDPOINT, etc.)
  - [x] Deploy secrets (DEPLOY_HOST, DEPLOY_USER, DEPLOY_KEY)
- [x] Create version bump script (`scripts/version.sh`)
- [x] Successful release build completed (January 2026)
- [ ] Clean up Azure debug code in release.yml (optional)

### 8.5 System Operations
- [x] Create `commands/system.rs`
  - [x] `show_in_folder` command (macOS, Windows, Linux)
  - [x] `open_external` command
  - [x] `get_app_version` command
  - [x] `get_platform_info` command
- [x] Register commands in lib.rs
- [x] Create `apps/desktop/src/lib/system.ts` (frontend client)
- [x] Create `packages/stores/src/windowState.ts`
- [x] Create `apps/desktop/src/lib/windowState.ts` (client using tauri-plugin-store)
- [x] Implement window state persistence (save/restore position, size, maximized, fullscreen)
- [x] Integrate theme detection with app theme (listens for system theme changes)
- [x] Tray icon implementation (system tray with Show/Quit menu)

### 8.6 DOCX Import ✅ COMPLETE
- [x] Create `services/docx_import.rs` (~850 lines)
  - [x] Parse DOCX structure (zip + quick-xml)
  - [x] Convert paragraphs to Tiptap JSON
  - [x] Handle headings and styles (H1-H6)
  - [x] Tables converted to text with warning
  - [x] Extract and save images (word/media/ with relationship mapping)
  - [x] Handle lists (bullet, numbered, nested)
  - [x] Text formatting (bold, italic, underline, strike, color, highlight, font size/family)
- [x] Add Tauri commands (`import_select_docx_file`, `import_analyze_docx`, `import_docx_file`)
- [x] Create `DocxImportDialog.svelte` (dedicated UI, not ImportWizard)
- [x] Add menu integration (File > Import Word Document...)
- [x] TypeScript types and client functions in `import.ts`

---

## Files to Create/Modify

### Created Files (Done)
```
apps/desktop/src-tauri/src/
├── commands/updates.rs          ✅ Created
├── commands/system.rs           ✅ Created
└── menu.rs                      ✅ Created

apps/desktop/src/lib/
├── components/UpdateDialog.svelte  ✅ Created
├── updates.ts                      ✅ Created (updates client)
├── system.ts                       ✅ Created (system client)
└── windowState.ts                  ✅ Created (window state client)

packages/stores/src/
├── updates.ts                   ✅ Created
└── windowState.ts               ✅ Created

.github/workflows/
├── check.yml                    ✅ Created
└── release.yml                  ✅ Created

apps/desktop/src-tauri/
└── entitlements.mac.plist       ✅ Copied from Electron app

scripts/
└── version.sh                   ✅ Created (version bump script)
```

### Modified Files (Done)
```
apps/desktop/src-tauri/
├── src/lib.rs                   ✅ Menu setup, new commands registered
├── src/commands/mod.rs          ✅ Export updates and system modules
└── tauri.conf.json              ✅ Updater config (pubkey, endpoints)

apps/desktop/src/
└── App.svelte                   ✅ Menu event listeners, updates integration

packages/stores/src/
└── index.ts                     ✅ Export updates store
```

### Files Created for DOCX Import (Complete)
```
apps/desktop/src-tauri/src/
└── services/docx_import.rs      ✅ DOCX import service (~850 lines)

apps/desktop/src/lib/
├── import.ts                    ✅ DOCX types and client methods added
└── components/
    └── DocxImportDialog.svelte  ✅ Dedicated DOCX import dialog

apps/desktop/src-tauri/src/
├── commands/import.rs           ✅ DOCX commands added
├── menu.rs                      ✅ "Import Word Document..." menu item
└── Cargo.toml                   ✅ Added zip + quick-xml dependencies
```

### Files Still To Modify (Optional Polish)
```
.github/workflows/
└── release.yml                  ⬜ Remove Azure debug code (lines 127-234)
```

### Note: Icons Already Present
```
apps/desktop/src-tauri/icons/    ✅ All icons present
├── 32x32.png
├── 64x64.png
├── 128x128.png
├── 128x128@2x.png
├── icon.icns                    (463KB - macOS)
├── icon.ico                     (173KB - Windows)
├── icon.png                     (305KB - general)
├── Square*.png                  (NSIS installer icons)
├── ios/                         (iOS icons)
└── android/                     (Android icons)
```

---

## Risk Mitigation

### Risk: Tauri Update Format Differences
**Mitigation:** Tauri uses JSON manifests vs Electron's YAML. Both can coexist in `/var/www/midlight-releases/` with different file names (`tauri-latest.json` vs `latest-mac.yml`).

### Risk: Azure Trusted Signing with Tauri
**Mitigation:** Test the `signCommand` approach locally first. If issues arise, fall back to post-build signing in the workflow (proven pattern from Electron).

### Risk: Platform-Specific Bugs
**Mitigation:** Test on all three platforms before each release. Use GitHub Actions matrix builds to catch issues early.

### Risk: Breaking Updates
**Mitigation:** Use test tags (e.g., `v0.1.0-beta.1`) for testing before production releases. Can also test updates with a staging update endpoint.

### Risk: Version Conflicts with Electron App
**Mitigation:** Use `midlight-next-` prefix for all Tauri artifacts to avoid filename collisions. Tauri app uses `tauri-latest.json` manifest, Electron uses `latest-mac.yml`.

---

## Success Criteria

Phase 8 is complete when:

1. ✅ **Auto-Updates:** Code complete - UpdateDialog, commands, store, client with periodic checks
2. ✅ **Native Menus:** macOS native menu bar implemented; Windows/Linux use custom menu
3. ✅ **Code Signing:** release.yml configured for Apple notarization + Azure Trusted Signing
4. ✅ **CI/CD:** check.yml + release.yml with all 3 platforms, deploy, manifest generation
5. ✅ **System Ops:** show_in_folder, open_external, window state persistence all implemented
6. ✅ **Distribution:** Secrets added, successful build completed (January 2026)
7. ✅ **DOCX Import:** Full implementation with parser, Tiptap conversion, images, UI dialog

**🎉 Phase 8 FULLY COMPLETE!**

All features implemented including polish items:
- [x] Clean up Azure debug code in release.yml
- [x] Theme detection integration (real-time system theme listening)
- [x] Tray icon implementation (Show Midlight, Quit menu)
- [ ] Confirm auto-update flow works end-to-end (after public release)

---

## Advantages of Leveraging Existing Infrastructure

| Aspect | Starting Fresh | Leveraging Existing |
|--------|----------------|---------------------|
| **Certificates** | $99/yr Apple + $200-500/yr Windows | Already paid, credentials in GitHub |
| **Signing Setup** | 2-3 days for Apple notarization | Copy config, test immediately |
| **Windows Signing** | Hardware token or expensive EV | Azure Trusted Signing (cloud-based, done) |
| **Deployment** | New infra or GitHub Releases | SCP to existing server |
| **Domain/SSL** | Need CDN setup | Caddy auto-SSL at midlight.ai |
| **Monitoring** | Set up from scratch | PM2 + existing logs |

**Estimated time saved: 1-2 weeks of infrastructure setup**
