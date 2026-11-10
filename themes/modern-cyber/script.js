// Connect to Modern RO Patcher Bridge API
document.addEventListener("DOMContentLoaded", function() {
    const statusText = document.getElementById("statusText");
    const percentText = document.getElementById("percentText");
    const progressFill = document.getElementById("progressFill");
    const detailFile = document.getElementById("detailFile");
    const detailSpeed = document.getElementById("detailSpeed");
    const btnPlay = document.getElementById("btnPlay");
    const serverTitle = document.getElementById("serverTitle");

    if (window.PatcherAPI) {
        if (window.PatcherAPI.config) {
            if (window.PatcherAPI.config.title && serverTitle) {
                serverTitle.innerText = window.PatcherAPI.config.title.replace(/\s*-\s*Patcher$/i, "");
            }
            if (window.PatcherAPI.config.notice_url) {
                const noticeItems = document.querySelectorAll(".notice-item");
                if (noticeItems.length > 1) {
                    noticeItems[1].setAttribute("href", window.PatcherAPI.config.notice_url);
                    noticeItems[1].setAttribute("target", "_blank");
                }
            }
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
