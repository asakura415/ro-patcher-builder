// ==========================================
// Sakura Anime Theme - Patcher Bridge & Effects
// ==========================================

document.addEventListener("DOMContentLoaded", function () {
    const statusText = document.getElementById("statusText");
    const percentText = document.getElementById("percentText");
    const progressFill = document.getElementById("progressFill");
    const detailFile = document.getElementById("detailFile");
    const detailSpeed = document.getElementById("detailSpeed");
    const btnPlay = document.getElementById("btnPlay");
    const serverTitle = document.getElementById("serverTitle");
    const noticeLinkItem = document.getElementById("noticeLinkItem");
    const btnSoundToggle = document.getElementById("btnSoundToggle");

    // ------------------------------------------
    // 1. Web Audio UI Synthesizer (Anime SFX)
    // ------------------------------------------
    let audioCtx = null;
    let soundEnabled = localStorage.getItem("anime-patcher-sound") !== "muted";

    function updateSoundIcon() {
        if (btnSoundToggle) {
            btnSoundToggle.innerHTML = soundEnabled ? "&#128266;" : "&#128263;";
            btnSoundToggle.title = soundEnabled ? "Mute Sound" : "Enable Sound";
        }
    }
    updateSoundIcon();

    if (btnSoundToggle) {
        btnSoundToggle.addEventListener("click", () => {
            soundEnabled = !soundEnabled;
            localStorage.setItem("anime-patcher-sound", soundEnabled ? "enabled" : "muted");
            updateSoundIcon();
            if (soundEnabled) playChime(880, 0.08);
        });
    }

    function initAudio() {
        if (!audioCtx && (window.AudioContext || window.webkitAudioContext)) {
            audioCtx = new (window.AudioContext || window.webkitAudioContext)();
        }
    }

    function playChime(freq = 660, duration = 0.12, type = "sine") {
        if (!soundEnabled) return;
        try {
            initAudio();
            if (!audioCtx) return;
            if (audioCtx.state === "suspended") {
                audioCtx.resume();
            }

            const osc = audioCtx.createOscillator();
            const gain = audioCtx.createGain();

            osc.type = type;
            osc.frequency.setValueAtTime(freq, audioCtx.currentTime);
            osc.frequency.exponentialRampToValueAtTime(freq * 1.5, audioCtx.currentTime + duration);

            gain.gain.setValueAtTime(0.06, audioCtx.currentTime);
            gain.gain.exponentialRampToValueAtTime(0.001, audioCtx.currentTime + duration);

            osc.connect(gain);
            gain.connect(audioCtx.destination);

            osc.start();
            osc.stop(audioCtx.currentTime + duration);
        } catch (e) {
            console.debug("Audio error:", e);
        }
    }

    function playSuccessFanfare() {
        if (!soundEnabled) return;
        const notes = [523.25, 659.25, 783.99, 1046.50]; // C5, E5, G5, C6
        notes.forEach((freq, i) => {
            setTimeout(() => playChime(freq, 0.22, "triangle"), i * 90);
        });
    }

    // Bind sounds to interactive elements
    document.querySelectorAll(".sound-click, .btn, .notice-item, .win-btn").forEach((el) => {
        el.addEventListener("mouseenter", () => playChime(750, 0.06, "sine"));
        el.addEventListener("click", () => playChime(920, 0.09, "triangle"));
    });

    // ------------------------------------------
    // 2. Falling Sakura Petals Animation
    // ------------------------------------------
    const canvas = document.getElementById("sakuraCanvas");
    if (canvas) {
        const ctx = canvas.getContext("2d");
        let width = (canvas.width = window.innerWidth);
        let height = (canvas.height = window.innerHeight);

        window.addEventListener("resize", () => {
            width = canvas.width = window.innerWidth;
            height = canvas.height = window.innerHeight;
        });

        const petalCount = 28;
        const petals = [];

        class Petal {
            constructor() {
                this.reset(true);
            }

            reset(initial = false) {
                this.x = Math.random() * width;
                this.y = initial ? Math.random() * height : -20;
                this.size = Math.random() * 8 + 8; // 8 - 16px
                this.speedY = Math.random() * 1.2 + 0.8;
                this.speedX = Math.random() * 0.8 + 0.3;
                this.swaySpeed = Math.random() * 0.02 + 0.01;
                this.swayAngle = Math.random() * Math.PI * 2;
                this.rotation = Math.random() * Math.PI * 2;
                this.rotSpeed = (Math.random() - 0.5) * 0.03;
                this.opacity = Math.random() * 0.45 + 0.45;
                this.flip = Math.random() * Math.PI;
                this.flipSpeed = Math.random() * 0.03 + 0.01;
            }

            update() {
                this.swayAngle += this.swaySpeed;
                this.rotation += this.rotSpeed;
                this.flip += this.flipSpeed;
                this.y += this.speedY;
                this.x += Math.sin(this.swayAngle) * 0.8 + this.speedX;

                if (this.y > height + 20 || this.x > width + 20) {
                    this.reset(false);
                }
            }

            draw() {
                ctx.save();
                ctx.translate(this.x, this.y);
                ctx.rotate(this.rotation);
                ctx.scale(Math.cos(this.flip), 1);
                ctx.fillStyle = `rgba(255, 183, 197, ${this.opacity})`;
                ctx.shadowColor = "rgba(255, 117, 140, 0.4)";
                ctx.shadowBlur = 4;

                // Draw realistic sakura petal shape
                ctx.beginPath();
                ctx.moveTo(0, -this.size / 2);
                ctx.bezierCurveTo(
                    this.size / 2, -this.size / 2,
                    this.size / 2, this.size / 2,
                    0, this.size
                );
                ctx.bezierCurveTo(
                    -this.size / 2, this.size / 2,
                    -this.size / 2, -this.size / 2,
                    0, -this.size / 2
                );
                ctx.closePath();
                ctx.fill();
                ctx.restore();
            }
        }

        for (let i = 0; i < petalCount; i++) {
            petals.push(new Petal());
        }

        let animationFrameId;
        function animate() {
            ctx.clearRect(0, 0, width, height);
            for (let i = 0; i < petals.length; i++) {
                petals[i].update();
                petals[i].draw();
            }
            animationFrameId = requestAnimationFrame(animate);
        }
        animate();
    }

    // ------------------------------------------
    // 3. Patcher API Bridge Integration
    // ------------------------------------------
    if (window.PatcherAPI) {
        if (window.PatcherAPI.config) {
            if (window.PatcherAPI.config.title && serverTitle) {
                serverTitle.innerText = window.PatcherAPI.config.title.replace(/\s*-\s*Patcher$/i, "");
            }
            if (window.PatcherAPI.config.notice_url && noticeLinkItem) {
                noticeLinkItem.setAttribute("href", window.PatcherAPI.config.notice_url);
            }
        }

        window.PatcherAPI.onStatusChange = function (status, data) {
            console.log("[SakuraTheme] Patcher Status:", status, data);

            switch (status) {
                case "checking":
                    statusText.innerText = "กำลังตรวจสอบการอัปเดต... (Checking updates)";
                    detailFile.innerText = "เชื่อมต่อไปยัง Patch Server...";
                    detailSpeed.innerText = "";
                    break;

                case "downloading":
                    const percent = data.total > 0 ? Math.round((data.current / data.total) * 100) : 0;
                    statusText.innerText = `กำลังดาวน์โหลดแพตช์ (${data.current}/${data.total})`;
                    percentText.innerText = `${percent}%`;
                    progressFill.style.width = `${percent}%`;
                    detailFile.innerText = data.current_file || "Downloading patch stream...";
                    detailSpeed.innerText = data.speed || "";
                    break;

                case "installing":
                    const installPercent = data.total > 0 ? Math.round((data.current / data.total) * 100) : 0;
                    statusText.innerText = `กำลังติดตั้งแพตช์ลงในเกม (${data.current}/${data.total})`;
                    percentText.innerText = `${installPercent}%`;
                    progressFill.style.width = `${installPercent}%`;
                    detailFile.innerText = data.file || "Writing into game archives...";
                    detailSpeed.innerText = "";
                    break;

                case "up_to_date":
                    statusText.innerText = "ตัวเกมอัปเดตสมบูรณ์ พร้อมเล่นแล้ว! (Ready to Play)";
                    percentText.innerText = "100%";
                    progressFill.style.width = "100%";
                    detailFile.innerText = "ไฟล์ทั้งหมดได้รับการอัปเดตเรียบร้อยแล้ว";
                    detailSpeed.innerText = "";
                    btnPlay.disabled = false;
                    btnPlay.classList.add("pulse");
                    playSuccessFanfare();
                    break;

                case "self_updated":
                    statusText.innerText = "Patcher ได้รับการอัปเดตใหม่ กำลังรีสตาร์ท...";
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
                    statusText.innerText = "เกิดข้อผิดพลาดในการอัปเดต (Update Error)";
                    progressFill.style.background = "linear-gradient(90deg, #e53e3e, #f56565)";
                    detailFile.innerText = data.error || "An error occurred during update.";
                    detailSpeed.innerText = "Failed";
                    break;

                case "idle":
                default:
                    statusText.innerText = "พร้อมเข้าเล่นเกม (Ready)";
                    break;
            }
        };
    }
});
