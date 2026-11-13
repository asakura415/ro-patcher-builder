// Connect to Modern RO Patcher Bridge API
document.addEventListener("DOMContentLoaded", function() {
    const statusText = document.getElementById("statusText");
    const percentText = document.getElementById("percentText");
    const progressFill = document.getElementById("progressFill");
    const detailFile = document.getElementById("detailFile");
    const detailSpeed = document.getElementById("detailSpeed");
    const btnPlay = document.getElementById("btnPlay");
    const serverTitle = document.getElementById("serverTitle");
    const btnAdminMode = document.getElementById("btnAdminMode");
    const adminModeText = document.getElementById("adminModeText");

    function updateAdminBadge(isAdmin) {
        if (!btnAdminMode || !adminModeText) return;
        if (isAdmin) {
            btnAdminMode.classList.remove("user-mode");
            btnAdminMode.classList.add("admin-mode");
            btnAdminMode.setAttribute("aria-disabled", "true");
            btnAdminMode.title = "กำลังทำงานด้วยสิทธิ์ผู้ดูแลระบบ (Running as Administrator)";
            adminModeText.innerText = "Admin Mode (สิทธิ์เต็ม)";
        } else {
            btnAdminMode.classList.remove("admin-mode");
            btnAdminMode.classList.add("user-mode");
            btnAdminMode.removeAttribute("aria-disabled");
            btnAdminMode.title = "คลิกเพื่อขอสิทธิ์ผู้ดูแลระบบ (Run as Administrator)";
            adminModeText.innerText = "User Mode (คลิกเพื่อขอ Admin)";
        }
    }

    if (btnAdminMode) {
        btnAdminMode.addEventListener("click", () => {
            const isAdmin = window.PatcherAPI && window.PatcherAPI.config && window.PatcherAPI.config.is_admin;
            if (!isAdmin && window.PatcherAPI && window.PatcherAPI.requestAdmin) {
                window.PatcherAPI.requestAdmin();
            }
        });
    }

    if (window.PatcherAPI) {
        if (window.PatcherAPI.config) {
            if (window.PatcherAPI.config.title && serverTitle) {
                serverTitle.innerText = window.PatcherAPI.config.title.replace(/\s*-\s*Patcher$/i, "");
            }
            const rawNoticeUrl = typeof window.PatcherAPI.config.notice_url === "string"
                ? window.PatcherAPI.config.notice_url.trim()
                : "";
            const noticeLinkItem = document.getElementById("noticeLinkItem") || document.querySelectorAll(".notice-item")[1];
            if (rawNoticeUrl && noticeLinkItem) {
                const normalizedNoticeUrl = /^[a-z][a-z0-9+.-]*:/i.test(rawNoticeUrl)
                    ? rawNoticeUrl
                    : `https://${rawNoticeUrl}`;
                if (/^https?:\/\//i.test(normalizedNoticeUrl)) {
                    noticeLinkItem.setAttribute("href", normalizedNoticeUrl);
                    noticeLinkItem.setAttribute("target", "_blank");
                    noticeLinkItem.setAttribute("rel", "noopener noreferrer");
                }
            }
            updateAdminBadge(Boolean(window.PatcherAPI.config.is_admin));
        }

        window.PatcherAPI.onStatusChange = function(status, data) {
            console.log("Patcher Status:", status, data);

            switch (status) {
                case "checking":
                    statusText.innerText = "Checking for new patches...";
                    detailFile.innerText = "Connecting to patch server...";
                    detailSpeed.innerText = "";
                    break;

                case "downloading":
                    const percent = data.total > 0 ? Math.round((data.current / data.total) * 100) : 0;
                    statusText.innerText = `Downloading updates (${data.current}/${data.total})`;
                    percentText.innerText = `${percent}%`;
                    progressFill.style.width = `${percent}%`;
                    detailFile.innerText = data.current_file || "Downloading patch stream...";
                    detailSpeed.innerText = data.speed || "";
                    break;

                case "installing":
                    const installPercent = data.total > 0 ? Math.round((data.current / data.total) * 100) : 0;
                    statusText.innerText = `Applying patches to game (${data.current}/${data.total})`;
                    percentText.innerText = `${installPercent}%`;
                    progressFill.style.width = `${installPercent}%`;
                    detailFile.innerText = data.file || "Writing into game archives...";
                    detailSpeed.innerText = "";
                    break;

                case "up_to_date":
                    statusText.innerText = "Game is fully updated and ready!";
                    percentText.innerText = "100%";
                    progressFill.style.width = "100%";
                    detailFile.innerText = "All patches applied successfully.";
                    detailSpeed.innerText = "";
                    btnPlay.disabled = false;
                    btnPlay.classList.add("pulse");
                    break;

                case "self_updated":
                    statusText.innerText = "Patcher updated! Restarting...";
                    percentText.innerText = "100%";
                    progressFill.style.width = "100%";
                    detailFile.innerText = "The patcher was updated to a newer version.";
                    setTimeout(() => {
                        if (window.PatcherAPI && window.PatcherAPI.restart) {
                            window.PatcherAPI.restart();
                        } else if (window.PatcherAPI) {
                            window.PatcherAPI.close();
                        }
                    }, 1500);
                    break;

                case "error":
                    statusText.innerText = "Update Error";
                    progressFill.style.background = "#e53e3e";
                    detailFile.innerText = data.error || "An error occurred during update.";
                    detailSpeed.innerText = "Failed";
                    break;

                case "idle":
                default:
                    statusText.innerText = "Ready";
                    break;
            }
        };
    }
});
