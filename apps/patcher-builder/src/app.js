// Modern RO Patcher Builder Frontend Logic
document.addEventListener("DOMContentLoaded", async function() {
    // Check Tauri API (supports both Global Tauri v2 and Tauri Internals IPC)
    const invoke = (window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.invoke)
        ? window.__TAURI__.core.invoke
        : (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)
            ? window.__TAURI_INTERNALS__.invoke
            : async (cmd, args) => {
                throw new Error(`Tauri IPC runtime is not available when invoking '${cmd}'`);
            };

    // Tab Navigation
    const navItems = document.querySelectorAll(".nav-item");
    const tabPanes = document.querySelectorAll(".tab-pane");
    const pageTitle = document.getElementById("pageTitle");
    const pageDesc = document.getElementById("pageDesc");

    function updateTabHeader(tabId) {
        if (!pageTitle || !pageDesc) return;
        if (window.I18N) {
            switch (tabId) {
                case "tab-server":
                    pageTitle.innerText = I18N.t("tabServerTitle");
                    pageDesc.innerText = I18N.t("tabServerDesc");
                    break;
                case "tab-client":
                    pageTitle.innerText = I18N.t("tabClientTitle");
                    pageDesc.innerText = I18N.t("tabClientDesc");
                    break;
                case "tab-theme":
                    pageTitle.innerText = I18N.t("tabThemeTitle");
                    pageDesc.innerText = I18N.t("tabThemeDesc");
                    break;
                case "tab-maker":
                    pageTitle.innerText = I18N.t("tabMakerTitle");
                    pageDesc.innerText = I18N.t("tabMakerDesc");
                    break;
                case "tab-generate":
                    pageTitle.innerText = I18N.t("tabGenerateTitle");
                    pageDesc.innerText = I18N.t("tabGenerateDesc");
                    break;
            }
        }
    }

    navItems.forEach(item => {
        item.addEventListener("click", () => {
            const tabId = item.getAttribute("data-tab");
            navItems.forEach(i => i.classList.remove("active"));
            tabPanes.forEach(p => p.classList.remove("active"));

            item.classList.add("active");
            const targetPane = document.getElementById(tabId);
            if (targetPane) targetPane.classList.add("active");

            updateTabHeader(tabId);
        });
    });

    // Slider Streams
    const dlSlider = document.getElementById("concurrentDl");
    const dlValue = document.getElementById("dlValue");
    if (dlSlider) {
        dlSlider.addEventListener("input", (e) => {
            dlValue.innerText = `${e.target.value} streams`;
        });
    }

    // Dynamic Mirrors
    let mirrors = [];
    const btnAddMirror = document.getElementById("btnAddMirror");
    const mirrorList = document.getElementById("mirrorList");

    function renderMirrors() {
        mirrorList.innerHTML = "";
        mirrors.forEach((m, idx) => {
            const div = document.createElement("div");
            div.className = "mirror-item";
            div.innerHTML = `
                <input type="text" placeholder="Mirror Name" value="${m.name}" data-idx="${idx}" class="mirror-name">
                <input type="text" placeholder="plist.txt URL" value="${m.plist_url}" data-idx="${idx}" class="mirror-plist">
                <input type="text" placeholder="Patch URL Directory" value="${m.patch_url}" data-idx="${idx}" class="mirror-patch">
                <button class="btn btn-outline btn-sm remove-mirror" data-idx="${idx}">&times;</button>
            `;
            mirrorList.appendChild(div);
        });

        // Add listeners
        document.querySelectorAll(".remove-mirror").forEach(btn => {
            btn.addEventListener("click", (e) => {
                const idx = parseInt(e.target.getAttribute("data-idx"));
                mirrors.splice(idx, 1);
                renderMirrors();
            });
        });

        document.querySelectorAll(".mirror-name").forEach(input => {
            input.addEventListener("input", (e) => {
                const idx = parseInt(e.target.getAttribute("data-idx"));
                mirrors[idx].name = e.target.value;
            });
        });
        document.querySelectorAll(".mirror-plist").forEach(input => {
            input.addEventListener("input", (e) => {
                const idx = parseInt(e.target.getAttribute("data-idx"));
                mirrors[idx].plist_url = e.target.value;
            });
        });
        document.querySelectorAll(".mirror-patch").forEach(input => {
            input.addEventListener("input", (e) => {
                const idx = parseInt(e.target.getAttribute("data-idx"));
                mirrors[idx].patch_url = e.target.value;
            });
        });
    }

    if (btnAddMirror) {
        btnAddMirror.addEventListener("click", () => {
            mirrors.push({
                name: `Mirror ${mirrors.length + 1}`,
                plist_url: "",
                patch_url: ""
            });
            renderMirrors();
        });
    }

    // Load Default Config
    async function loadDefaults() {
        try {
            const defaultCfg = await invoke("get_default_config");
            if (defaultCfg) {
                if (defaultCfg.server && defaultCfg.server.patch_servers.length > 0) {
                    const primary = defaultCfg.server.patch_servers[0];
                    document.getElementById("srvName").value = primary.name;
                    document.getElementById("srvPlist").value = primary.plist_url;
                    document.getElementById("srvPatch").value = primary.patch_url;
                }
                if (defaultCfg.client) {
                    document.getElementById("clientExe").value = defaultCfg.client.executable;
                    document.getElementById("defaultGrf").value = defaultCfg.client.default_grf_name;
                    if (defaultCfg.client.setup_executable) {
                        document.getElementById("setupExe").value = defaultCfg.client.setup_executable;
                    }
                    if (defaultCfg.client.arguments) {
                        document.getElementById("clientArgs").value = defaultCfg.client.arguments.join(" ");
                    }
                }
                if (defaultCfg.window) {
                    document.getElementById("winTitle").value = defaultCfg.window.title;
                    document.getElementById("winWidth").value = defaultCfg.window.width;
                    document.getElementById("winHeight").value = defaultCfg.window.height;
                    document.getElementById("winTransparent").checked = defaultCfg.window.transparent;
                    document.getElementById("winResizable").checked = defaultCfg.window.resizable;
                }
            }
        } catch (e) {
            console.error("Failed loading default config:", e);
        }
    }

    document.getElementById("btnResetConfig").addEventListener("click", loadDefaults);
    await loadDefaults();

    // Auto-Scan Game Directory Modal
    const scanModal = document.getElementById("scanModal");
    const btnScanGame = document.getElementById("btnScanGame");
    const closeScanModal = document.getElementById("closeScanModal");
    const btnCancelScan = document.getElementById("btnCancelScan");
    const btnDoScan = document.getElementById("btnDoScan");
    const scanDirInput = document.getElementById("scanDirInput");
    const scanResults = document.getElementById("scanResults");

    btnScanGame.addEventListener("click", () => scanModal.classList.add("active"));
    closeScanModal.addEventListener("click", () => scanModal.classList.remove("active"));
    btnCancelScan.addEventListener("click", () => scanModal.classList.remove("active"));

    btnDoScan.addEventListener("click", async () => {
        const path = scanDirInput.value.trim();
        if (!path) return;

        btnDoScan.innerText = "Scanning...";
        btnDoScan.disabled = true;

        try {
            const res = await invoke("scan_game_directory", { dirPath: path });
            scanResults.style.display = "block";
            let html = `<strong>Scan Results in ${path}:</strong><br>`;
            html += `&#127918; Client Executable: <b>${res.found_clients.join(", ") || "None"}</b><br>`;
            html += `&#9881; Settings Executable: <b>${res.found_settings.join(", ") || "None"}</b><br>`;
            html += `&#128230; GRF Archives: <b>${res.found_grfs.join(", ") || "None"}</b><br>`;
            html += `&#127912; Icons: <b>${res.found_icons.join(", ") || "None"}</b><br>`;
            scanResults.innerHTML = html;

            // Auto-fill form fields
            if (res.found_clients.length > 0) {
                document.getElementById("clientExe").value = res.found_clients[0];
                document.getElementById("genOutputPath").value = `${path}\\${res.found_clients[0].replace(".exe", " - Patcher.exe")}`;
            }
            if (res.found_settings.length > 0) {
                document.getElementById("setupExe").value = res.found_settings[0];
            }
            if (res.found_grfs.length > 0) {
                const preferredGrf = res.found_grfs.find(g => g.startsWith("_") && g.includes("box")) || res.found_grfs[0];
                document.getElementById("defaultGrf").value = preferredGrf;
                document.getElementById("makerTargetGrf").value = preferredGrf;
            }
            if (res.found_icons.length > 0) {
                document.getElementById("customIcon").value = `${path}\\${res.found_icons[0]}`;
            }

            btnDoScan.innerText = "Applied to Config!";
            setTimeout(() => {
                scanModal.classList.remove("active");
                btnDoScan.innerText = "Scan Folder";
                btnDoScan.disabled = false;
            }, 1200);
        } catch (err) {
            alert("Error scanning directory: " + err);
            btnDoScan.innerText = "Scan Folder";
            btnDoScan.disabled = false;
        }
    });

    // --- Patch Maker Sub-Tabs, Input Modes, and Delete List ---
    const subtabButtons = document.querySelectorAll(".maker-subtab");
    const subPanes = document.querySelectorAll(".maker-subpane");

    subtabButtons.forEach(btn => {
        btn.addEventListener("click", () => {
            const targetId = btn.getAttribute("data-subtab");
            subtabButtons.forEach(b => {
                b.classList.remove("active");
                b.setAttribute("aria-selected", "false");
            });
            subPanes.forEach(p => {
                p.classList.remove("active");
                p.hidden = true;
            });

            btn.classList.add("active");
            btn.setAttribute("aria-selected", "true");
            const targetPane = document.getElementById(targetId);
            if (targetPane) {
                targetPane.classList.add("active");
                targetPane.hidden = false;
            }
        });
    });

    // Input Mode Radio Switching (Directory / Single File / Patcher & Game EXE)
    const inputModeRadios = document.querySelectorAll('input[name="makerInputMode"]');
    const labelMakerInput = document.getElementById("labelMakerInput");
    const makerInputDirEl = document.getElementById("makerInputDir");
    const btnMakerBrowseInput = document.getElementById("btnMakerBrowseInput");
    const singleFileTargetGroup = document.getElementById("singleFileTargetGroup");
    const makerUseGrfEl = document.getElementById("makerUseGrf");

    inputModeRadios.forEach(radio => {
        radio.addEventListener("change", () => {
            const mode = radio.value;
            if (mode === "directory") {
                if (btnMakerBrowseInput) btnMakerBrowseInput.setAttribute("data-browse-mode", "folder");
                if (labelMakerInput) {
                    labelMakerInput.setAttribute("data-i18n", "labelInputDir");
                    if (window.I18N) labelMakerInput.innerText = I18N.t("labelInputDir");
                }
                if (makerInputDirEl) makerInputDirEl.placeholder = "D:\\ro\\patches\\2026-11-update\\data";
                if (singleFileTargetGroup) singleFileTargetGroup.style.display = "none";
            } else if (mode === "single_file") {
                if (btnMakerBrowseInput) btnMakerBrowseInput.setAttribute("data-browse-mode", "file_any");
                if (labelMakerInput) {
                    labelMakerInput.setAttribute("data-i18n", "labelInputSingleFile");
                    if (window.I18N) labelMakerInput.innerText = I18N.t("labelInputSingleFile");
                }
                if (makerInputDirEl) makerInputDirEl.placeholder = "D:\\ro\\patches\\itemInfo.lub";
                if (singleFileTargetGroup) singleFileTargetGroup.style.display = "flex";
            } else if (mode === "patcher_exe") {
                if (btnMakerBrowseInput) btnMakerBrowseInput.setAttribute("data-browse-mode", "file_exe");
                if (labelMakerInput) {
                    labelMakerInput.setAttribute("data-i18n", "labelInputExeFile");
                    if (window.I18N) labelMakerInput.innerText = I18N.t("labelInputExeFile");
                }
                if (makerInputDirEl) makerInputDirEl.placeholder = "D:\\ro\\PRONTERA BOX\\_PRONTERA BOX.exe";
                if (singleFileTargetGroup) singleFileTargetGroup.style.display = "none";
                if (makerUseGrfEl) makerUseGrfEl.checked = false;
            }
        });
    });

    // Delete File List State & Handlers
    let deleteFiles = [];
    const makerDeleteList = document.getElementById("makerDeleteList");
    const deleteCountBadge = document.getElementById("deleteCountBadge");
    const makerDeleteInput = document.getElementById("makerDeleteInput");
    const btnAddDeleteFile = document.getElementById("btnAddDeleteFile");
    const btnImportDeleteTxt = document.getElementById("btnImportDeleteTxt");
    const btnImportDeleteClip = document.getElementById("btnImportDeleteClip");
    const btnClearDeleteList = document.getElementById("btnClearDeleteList");

    function renderDeleteList() {
        if (!makerDeleteList) return;
        if (deleteCountBadge) {
            deleteCountBadge.innerText = String(deleteFiles.length);
        }
        makerDeleteList.innerHTML = "";
        if (deleteFiles.length === 0) {
            const emptyDiv = document.createElement("div");
            emptyDiv.className = "delete-list-empty";
            emptyDiv.setAttribute("data-i18n", "emptyDeleteList");
            emptyDiv.textContent = window.I18N ? I18N.t("emptyDeleteList") : "No files queued for deletion.";
            makerDeleteList.appendChild(emptyDiv);
            return;
        }

        deleteFiles.forEach((filePath, idx) => {
            const row = document.createElement("div");
            row.className = "delete-item";
            row.setAttribute("role", "listitem");

            const span = document.createElement("span");
            span.className = "delete-item-path";
            span.textContent = filePath;

            const removeBtn = document.createElement("button");
            removeBtn.type = "button";
            removeBtn.className = "delete-item-remove";
            removeBtn.setAttribute("aria-label", `Remove ${filePath}`);
            removeBtn.textContent = "\u00D7";
            removeBtn.addEventListener("click", () => {
                deleteFiles.splice(idx, 1);
                renderDeleteList();
            });

            row.appendChild(span);
            row.appendChild(removeBtn);
            makerDeleteList.appendChild(row);
        });
    }

    function addDeleteEntries(lines) {
        for (const raw of lines) {
            const trimmed = (raw || "").trim();
            if (!trimmed || trimmed.startsWith("#") || trimmed.startsWith("//")) continue;
            const normalized = trimmed.replace(/\//g, "\\").replace(/^\\+/, "");
            if (normalized && !deleteFiles.includes(normalized)) {
                deleteFiles.push(normalized);
            }
        }
        renderDeleteList();
    }

    if (btnAddDeleteFile && makerDeleteInput) {
        btnAddDeleteFile.addEventListener("click", () => {
            if (makerDeleteInput.value.trim()) {
                addDeleteEntries([makerDeleteInput.value]);
                makerDeleteInput.value = "";
                makerDeleteInput.focus();
            }
        });

        makerDeleteInput.addEventListener("keydown", (e) => {
            if (e.key === "Enter" && !e.isComposing) {
                e.preventDefault();
                if (makerDeleteInput.value.trim()) {
                    addDeleteEntries([makerDeleteInput.value]);
                    makerDeleteInput.value = "";
                }
            }
        });
    }

    if (btnImportDeleteTxt) {
        btnImportDeleteTxt.addEventListener("click", async () => {
            try {
                const imported = await invoke("import_delete_list_file");
                if (Array.isArray(imported) && imported.length > 0) {
                    addDeleteEntries(imported);
                }
            } catch (err) {
                alert("Failed to import delete list file: " + err);
            }
        });
    }

    if (btnImportDeleteClip) {
        btnImportDeleteClip.addEventListener("click", async () => {
            try {
                const clipText = await navigator.clipboard.readText();
                if (clipText) {
                    addDeleteEntries(clipText.split(/\r?\n/));
                }
            } catch (err) {
                alert("Unable to read clipboard: " + err);
            }
        });
    }

    if (btnClearDeleteList) {
        btnClearDeleteList.addEventListener("click", () => {
            deleteFiles = [];
            renderDeleteList();
        });
    }

    renderDeleteList();

    // Patch Maker Action
    const btnBuildThor = document.getElementById("btnBuildThor");
    const makerStatusCard = document.getElementById("makerStatusCard");
    const makerStatusTitle = document.getElementById("makerStatusTitle");
    const makerStatusMsg = document.getElementById("makerStatusMsg");

    btnBuildThor.addEventListener("click", async () => {
        const inputDir = document.getElementById("makerInputDir").value.trim();
        const outputThor = document.getElementById("makerOutputThor").value.trim();
        const targetGrf = document.getElementById("makerTargetGrf").value.trim();
        const useGrf = document.getElementById("makerUseGrf").checked;
        const plistPath = document.getElementById("makerPlistPath").value.trim();
        const singleTarget = document.getElementById("makerSingleTarget") ? document.getElementById("makerSingleTarget").value.trim() : "";

        const selectedInputMode = document.querySelector('input[name="makerInputMode"]:checked')?.value || "directory";
        const compLevel = parseInt(document.querySelector('input[name="makerCompLevel"]:checked')?.value || "9", 10);
        const encoding = document.querySelector('input[name="makerEncoding"]:checked')?.value || "unicode";

        const effectiveInputMode = (!inputDir && deleteFiles.length > 0) ? "delete_only" : selectedInputMode;

        if (!outputThor) {
            alert("Please specify the Output .thor file path.");
            return;
        }
        if (!inputDir && deleteFiles.length === 0) {
            alert("Please specify an Input source (Folder/File) or add at least one file to the Delete File list.");
            return;
        }

        btnBuildThor.innerText = "Packaging THOR Archive...";
        btnBuildThor.disabled = true;

        try {
            const res = await invoke("create_thor_patch", {
                req: {
                    input_dir: inputDir,
                    input_mode: effectiveInputMode,
                    single_file_target_path: singleTarget || null,
                    output_thor_path: outputThor,
                    use_grf_merging: useGrf,
                    target_grf: targetGrf || null,
                    include_checksums: true,
                    compression_level: compLevel,
                    filename_encoding: encoding,
                    delete_files: deleteFiles,
                    update_plist_path: plistPath || null,
                    forced_patch_index: null
                }
            });

            makerStatusCard.style.display = "flex";
            makerStatusCard.style.borderColor = "var(--accent-blue)";
            makerStatusTitle.innerText = "THOR Archive Created!";
            makerStatusMsg.innerText = res.message + (res.plist_index ? ` (plist.txt updated with index ${res.plist_index})` : "");
        } catch (e) {
            makerStatusCard.style.display = "flex";
            makerStatusCard.style.borderColor = "#e53e3e";
            makerStatusTitle.innerText = "Build Failed";
            makerStatusMsg.innerText = e;
        } finally {
            const labelText = window.I18N ? I18N.t("btnBuildThor") : "Generate THOR Patch (.thor)";
            btnBuildThor.innerHTML = `<span>&#128230;</span> <span data-i18n="btnBuildThor">${labelText}</span>`;
            btnBuildThor.disabled = false;
        }
    });

    // Generate Patcher Action
    const btnGenerate = document.getElementById("btnGeneratePatcher");
    const genBanner = document.getElementById("genResultBanner");
    const resTitle = document.getElementById("resTitle");
    const resDesc = document.getElementById("resDesc");
    const resMeta = document.getElementById("resMeta");

    btnGenerate.addEventListener("click", async () => {
        btnGenerate.innerText = "Generating Standalone Executable...";
        btnGenerate.disabled = true;
        genBanner.style.display = "none";

        // Collect all configs
        const patchServers = [
            {
                name: document.getElementById("srvName").value.trim() || "Main Server",
                plist_url: document.getElementById("srvPlist").value.trim(),
                patch_url: document.getElementById("srvPatch").value.trim()
            },
            ...mirrors.filter(m => m.plist_url && m.patch_url)
        ];

        const clientArgsStr = document.getElementById("clientArgs").value.trim();
        const clientArgs = clientArgsStr ? clientArgsStr.split(" ").filter(s => s.length > 0) : [];

        const config = {
            server: {
                patch_servers: patchServers,
                preferred_server: patchServers[0] ? patchServers[0].name : null,
                notice_url: document.getElementById("srvNotice").value.trim() || null
            },
            client: {
                executable: document.getElementById("clientExe").value.trim() || "ragexe.exe",
                arguments: clientArgs,
                setup_executable: document.getElementById("setupExe").value.trim() || null,
                default_grf_name: document.getElementById("defaultGrf").value.trim() || "data.grf"
            },
            patching: {
                in_place: document.getElementById("checkInPlace").checked,
                create_grf: document.getElementById("checkCreateGrf").checked,
                check_integrity: document.getElementById("checkIntegrity").checked,
                concurrent_downloads: parseInt(document.getElementById("concurrentDl").value) || 16
            },
            window: {
                title: document.getElementById("winTitle").value.trim() || "Ragnarok Online Patcher",
                width: parseInt(document.getElementById("winWidth").value) || 800,
                height: parseInt(document.getElementById("winHeight").value) || 500,
                resizable: document.getElementById("winResizable").checked,
                decorations: false,
                transparent: document.getElementById("winTransparent").checked
            }
        };

        const customIcon = document.getElementById("customIcon").value.trim();
        let customTheme = document.getElementById("customThemeFolder").value.trim();
        if (!customTheme) {
            const activeCard = document.querySelector(".theme-card.active");
            if (activeCard) {
                customTheme = activeCard.getAttribute("data-theme") || "";
            }
        }
        const outputPath = document.getElementById("genOutputPath").value.trim();
        const templatePath = document.getElementById("genTemplatePath").value.trim();

        if (!outputPath) {
            genBanner.style.display = "flex";
            genBanner.style.borderColor = "#e53e3e";
            resTitle.innerText = "Build Failed";
            resDesc.innerText = "Please specify an Output Executable Destination (.exe).";
            resMeta.innerText = "";
            btnGenerate.innerHTML = "<span>&#128640;</span> GENERATE STANDALONE PATCHER (.EXE)";
            btnGenerate.disabled = false;
            return;
        }

        try {
            const res = await invoke("generate_patcher", {
                req: {
                    config: config,
                    output_exe_path: outputPath,
                    template_exe_path: templatePath || null,
                    custom_icon_path: customIcon || null,
                    theme_folder_path: customTheme || null
                }
            });

            if (!res || !res.success) {
                throw new Error((res && res.message) ? res.message : "Failed to generate standalone patcher.");
            }

            genBanner.style.display = "flex";
            genBanner.style.borderColor = "#48bb78";
            resTitle.innerText = "Build Succeeded!";
            resDesc.innerText = res.message;
            resMeta.innerText = `Output File: ${res.output_path} | Size: ${(res.file_size_bytes / (1024 * 1024)).toFixed(2)} MB`;
        } catch (err) {
            genBanner.style.display = "flex";
            genBanner.style.borderColor = "#e53e3e";
            resTitle.innerText = "Build Failed";
            resDesc.innerText = err;
            resMeta.innerText = "";
        } finally {
            btnGenerate.innerHTML = "<span>&#128640;</span> GENERATE STANDALONE PATCHER (.EXE)";
            btnGenerate.disabled = false;
        }
    });

    // --- Theme Switcher (Dark / Light Mode) ---
    const btnThemeToggle = document.getElementById("btnThemeToggle");
    const themeIcon = document.getElementById("themeIcon");
    const themeLabel = document.getElementById("themeLabel");
    const colorSchemeMeta = document.querySelector('meta[name="color-scheme"]');

    function applyTheme(theme) {
        document.documentElement.setAttribute("data-theme", theme);
        if (colorSchemeMeta) {
            colorSchemeMeta.content = theme;
        }
        localStorage.setItem("color-scheme", theme);

        if (theme === "light") {
            themeIcon.innerHTML = "&#9728;&#65039;"; // Sun
            themeLabel.innerText = window.I18N ? I18N.t("themeLight") : "Light Mode";
            btnThemeToggle.setAttribute("aria-label", "Switch to Dark Mode");
        } else {
            themeIcon.innerHTML = "&#127769;"; // Moon
            themeLabel.innerText = window.I18N ? I18N.t("themeDark") : "Dark Mode";
            btnThemeToggle.setAttribute("aria-label", "Switch to Light Mode");
        }
    }

    // Initialize theme from storage
    const initialTheme = localStorage.getItem("color-scheme") || "dark";
    applyTheme(initialTheme);

    if (btnThemeToggle) {
        btnThemeToggle.addEventListener("click", () => {
            const currentTheme = document.documentElement.getAttribute("data-theme") || "dark";
            const nextTheme = currentTheme === "dark" ? "light" : "dark";
            applyTheme(nextTheme);
        });
    }

    // --- Language Switcher (EN / TH) ---
    const btnLangToggle = document.getElementById("btnLangToggle");
    if (btnLangToggle) {
        btnLangToggle.addEventListener("click", () => {
            if (window.I18N) {
                const currentLang = I18N.currentLang || "en";
                const nextLang = currentLang === "en" ? "th" : "en";
                I18N.setLanguage(nextLang);
                const activeNav = document.querySelector(".nav-item.active");
                if (activeNav) {
                    updateTabHeader(activeNav.getAttribute("data-tab"));
                }
                const currentTheme = document.documentElement.getAttribute("data-theme") || "dark";
                applyTheme(currentTheme);
            }
        });
    }

    // Initialize I18N
    if (window.I18N) {
        window.I18N.init();
    }

    // --- Theme Card Selection ---
    const themeCards = document.querySelectorAll(".theme-card");
    const customThemeFolderInput = document.getElementById("customThemeFolder");
    themeCards.forEach(card => {
        card.addEventListener("click", () => {
            themeCards.forEach(c => c.classList.remove("active"));
            card.classList.add("active");
            const themePath = card.getAttribute("data-theme");
            if (customThemeFolderInput) {
                customThemeFolderInput.value = themePath;
            }
        });
    });

    if (customThemeFolderInput) {
        customThemeFolderInput.addEventListener("input", () => {
            const val = customThemeFolderInput.value.trim().replace(/\\/g, "/");
            themeCards.forEach(card => {
                const cardTheme = card.getAttribute("data-theme");
                if (val === cardTheme || val.endsWith(cardTheme)) {
                    card.classList.add("active");
                } else {
                    card.classList.remove("active");
                }
            });
        });
    }

    // --- File / Folder Browse Buttons ---
    document.querySelectorAll(".btn-browse").forEach(btn => {
        btn.addEventListener("click", async () => {
            const targetId = btn.getAttribute("data-browse-target");
            const mode = btn.getAttribute("data-browse-mode") || "folder";
            const inputEl = targetId ? document.getElementById(targetId) : null;
            const currentPath = inputEl ? inputEl.value.trim() : "";
            const prevText = btn.innerText;
            btn.disabled = true;
            try {
                const selected = await invoke("pick_path", { mode, currentPath });
                if (typeof selected === "string" && selected.trim().length > 0 && inputEl) {
                    inputEl.value = selected.trim();
                    inputEl.dispatchEvent(new Event("input", { bubbles: true }));
                }
            } catch (err) {
                console.error("Browse dialog failed:", err);
                alert("Failed to open file/folder dialog: " + err);
            } finally {
                btn.disabled = false;
                btn.innerText = prevText;
            }
        });
    });

    // --- Info & User Guide Modal Dialog ---
    const infoDialog = document.getElementById("infoDialog");
    const btnOpenInfo = document.getElementById("btnOpenInfo");
    const btnCloseInfo = document.getElementById("btnCloseInfo");
    const btnCloseInfoFooter = document.getElementById("btnCloseInfoFooter");

    if (infoDialog && btnOpenInfo) {
        btnOpenInfo.addEventListener("click", () => {
            infoDialog.showModal();
        });

        if (btnCloseInfo) {
            btnCloseInfo.addEventListener("click", () => infoDialog.close());
        }

        if (btnCloseInfoFooter) {
            btnCloseInfoFooter.addEventListener("click", () => infoDialog.close());
        }

        // Light dismiss fallback: click outside dialog box closes it
        infoDialog.addEventListener("click", (e) => {
            const rect = infoDialog.getBoundingClientRect();
            const isInDialog = (
                rect.top <= e.clientY && e.clientY <= rect.top + rect.height &&
                rect.left <= e.clientX && e.clientX <= rect.left + rect.width
            );
            if (!isInDialog) {
                infoDialog.close();
            }
        });
    }

    // --- External Links Handler ---
    document.addEventListener("click", (e) => {
        const link = e.target.closest("a[data-external]");
        if (link) {
            e.preventDefault();
            const href = link.getAttribute("href");
            if (href) {
                invoke("open_external_url", { url: href }).catch(err => {
                    console.warn("open_external_url failed, falling back to window.open:", err);
                    window.open(href, "_blank", "noopener,noreferrer");
                });
            }
        }
    });
});
