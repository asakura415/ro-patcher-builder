// Modern RO Patcher Builder - Bilingual Dictionary (EN / TH)
const I18N = {
    currentLang: "en",

    translations: {
        en: {
            // General & Nav
            appName: "RO Patcher",
            appBadge: "BUILDER",
            navServer: "Server & Mirrors",
            navClient: "Game & GRF",
            navTheme: "UI Theme & Window",
            navMaker: "Patch Maker (.thor)",
            navGenerate: "Generate Patcher",
            btnScanGame: "Auto-Scan Game Folder",
            btnResetDefaults: "Reset Defaults",
            btnGuideInfo: "Guide & Info",
            langLabel: "English",
            themeDark: "Dark Mode",
            themeLight: "Light Mode",

            // Tab Titles & Descs
            tabServerTitle: "Server & Mirrors Configuration",
            tabServerDesc: "Configure your patch servers, mirror probes, and notice board.",
            tabClientTitle: "Game Client & GRF Settings",
            tabClientDesc: "Select target client executable, launch arguments, and GRF merging options.",
            tabThemeTitle: "UI Theme & Window Customization",
            tabThemeDesc: "Choose your HTML/CSS patcher theme, window size, and custom icon.",
            tabMakerTitle: "Patch Maker Tool (.thor)",
            tabMakerDesc: "Compress folder updates into standard .thor patches and update plist.txt.",
            tabGenerateTitle: "Generate Standalone Patcher",
            tabGenerateDesc: "Export single self-contained executable with embedded configs.",

            // Tab 1: Server
            cardPrimaryServer: "Primary Patch Server",
            labelServerName: "Server Name",
            labelPlistUrl: "Patch List URL (plist.txt)",
            labelPatchDir: "Patch Files URL Directory",
            hintPatchDir: "Must end with a trailing slash where .thor files are hosted.",
            labelNoticeUrl: "Notice / News URL (Optional)",
            cardMirrors: "Mirror Servers (Failover)",
            btnAddMirror: "+ Add Mirror",
            descMirrors: "If the primary server is unreachable, the patcher automatically probes mirrors in parallel.",

            // Tab 2: Client & GRF
            cardClientExec: "Client Executable & Launch Settings",
            labelClientExe: "Game Client Executable",
            labelSetupExe: "Setup / Settings Executable",
            labelLaunchArgs: "Launch Arguments",
            hintLaunchArgs: "Arguments required by your client or Gepard Shield / Anti-cheat.",
            cardGrfSettings: "GRF Archive Merging & Integrity",
            labelDefaultGrf: "Default Target GRF",
            labelConcurrentDl: "Concurrent Download Streams",
            toggleInplace: "In-Place GRF Merging",
            descInplace: "Fastest merging by appending new file tables without rewriting entire multi-gigabyte GRF.",
            toggleAutocreate: "Auto-Create GRF",
            descAutocreate: "Automatically initializes an empty GRF v2.0 if target GRF doesn't exist.",
            toggleIntegrity: "Verify Data Integrity",
            descIntegrity: "Enforces SHA-1 / Checksum validation on downloaded .thor packages before applying.",

            // Tab 3: UI Theme
            cardWindowCustom: "Window Customization",
            labelWinTitle: "Window Title",
            labelDimensions: "Dimensions (Width × Height)",
            toggleTransparent: "Transparent & Frameless Window",
            descTransparent: "Enables modern rounded corners and glassmorphism without Windows border titlebars.",
            toggleResizable: "Resizable Window",
            descResizable: "Allow players to resize patcher window.",
            labelCustomIcon: "Custom Patcher Icon (.ico)",
            hintCustomIcon: "Will be embedded into the PE binary before attaching the overlay.",
            cardThemeTemplate: "Web Theme Template",
            themeCyberTitle: "Modern Cyber Neon (Default)",
            themeCyberDesc: "Sleek glassmorphism, glowing neon accents, and smooth animated progress.",
            themeAnimeTitle: "Sakura Anime Fantasy",
            themeAnimeDesc: "Japanese anime fantasy style, floating sakura petals, sweet sound effects, and mascot artwork.",
            labelCustomThemeFolder: "Or Custom Web Theme Folder Path",
            bgmStatusDetectedTitle: "Background Music (BGM) Detected",
            bgmStatusDetectedDesc: "Found {path} ({size} MB) — will be embedded and played automatically with a mute toggle.",
            bgmStatusNoneTitle: "No Background Music (Optional)",
            bgmStatusNoneDesc: "assets/bgm.mp3 was not found in this theme — patcher will generate normally without music.",
            bgmStatusMissingTitle: "Theme Folder Not Found or Missing index.html",
            bgmStatusMissingDesc: "Please check the theme folder path before generating.",

            // Tab 4: Patch Maker
            cardCreateThor: "Create THOR Patch (.thor)",
            descCreateThor: "Package your data updates into standard THOR format with zlib compression, Korean CP949 encoding, and file deletion list.",
            labelInputDir: "Input Folder (Contains data/, System/, or files to patch)",
            labelInputSingleFile: "Input Single File Path",
            labelInputExeFile: "Input Patcher / Game Executable (.exe)",
            labelSingleTarget: "Target Relative Path in GRF / Client (Optional)",
            labelOutputThor: "Output .thor File Path",
            labelTargetGrf: "Target GRF Archive",
            hintTargetGrf: "Leave empty to use the Default GRF specified in Patcher config.",
            toggleMergeGrf: "Output Mode: RO - GRF (Merge into GRF)",
            descMergeGrf: "Uncheck for \"File\" mode to extract directly into the root game directory (e.g. System/*.lub, .exe, .dll).",
            labelPlistPath: "Auto-Update plist.txt (Optional)",
            subtabAddFile: "Add File",
            subtabDeleteFile: "Delete File",
            legendInputMode: "Input Source Mode",
            radioModeDir: "Directory",
            radioModeSingle: "Single File",
            radioModeExe: "Patcher & Game EXE",
            legendCompLevel: "Compression Level",
            radioCompLow: "Low (Fast)",
            radioCompMed: "Medium",
            radioCompHigh: "High (Best)",
            legendEncoding: "Options — Filenames in directory are...",
            radioEncUnicode: "Unicode (CP949)",
            radioEncAscii: "ASCII (Win-1252)",
            labelDeleteList: "List of files will be deleted",
            btnImportTxt: "Import .txt",
            btnImportClipboard: "From Clipboard",
            btnClearDelete: "Clear All",
            labelAddDelete: "Add to list (Target Relative Path)",
            btnAddDelete: "Add!",
            emptyDeleteList: "No files queued for deletion. Add paths below or import from a .txt file / clipboard.",
            btnBuildThor: "Generate THOR Patch (.thor)",

            // Tab 5: Generate
            heroGenerateTitle: "Generate Standalone Single Executable",
            heroGenerateDesc: "Compiles all server configurations, themes, and assets into a single self-contained .exe file ready for player distribution. No additional folders or yaml files needed.",
            labelGenOutput: "Output Executable Destination",
            labelGenTemplate: "Base Template Binary (patcher_base.exe)",
            btnGeneratePatcher: "GENERATE STANDALONE PATCHER (.EXE)",

            // Scan Modal
            scanModalTitle: "Auto-Scan Game Folder",
            scanModalPrompt: "Enter the path to your Ragnarok Online client directory:",
            btnCancelScan: "Cancel",
            btnDoScan: "Scan Folder",

            // Info Modal
            infoBadge: "USER GUIDE & MANUAL",
            infoTitle: "Modern RO Patcher Builder Guide & Documentation",
            infoCloseBtn: "Got it (Close)",
            infoSec1Title: "1. System Overview",
            infoSec1Text: "<strong>Modern RO Patcher Builder</strong> is a next-generation client patcher toolchain for Ragnarok Online built in <strong>Rust 2021 + Microsoft Edge WebView2</strong>.",
            infoTag1: "✔ Zero Anti-Virus False Positives",
            infoTag2: "✔ Single Self-Contained Executable",
            infoTag3: "✔ Multi-stream Downloader (up to 32 streams)",
            infoTag4: "✔ High-Speed In-Place GRF Merging",
            infoTag5: "✔ Binary Overlay Engine (ROPATCH1)",
            
            infoSec2Title: "2. Step-by-Step Workflow Guide",
            step1Title: "Server & Mirrors Configuration",
            step1Desc: "Specify the patch list URL (<code>plist.txt</code>) and the directory hosting your <code>.thor</code> files. Add multiple failover mirror servers for maximum download redundancy.",
            step2Title: "Game Client & GRF Settings",
            step2Desc: "Set your game client binary (e.g. <code>_PRONTERA BOX.exe</code>), launch parameters (<code>-1rag1</code>), settings tool (<code>_PRONTERA BOX - Settings.exe</code>), and primary GRF archive. Choose concurrent streams (1–32).",
            step3Title: "UI Theme & Window Customization",
            step3Desc: "Adjust window dimensions (width × height), frameless transparency, embed a custom PE icon (<code>.ico</code>), and select your Web UI theme folder.",
            step4Title: "Patch Maker (.thor Tool)",
            step4Desc: "Put updated game assets (e.g. <code>data/</code>) into an update folder. Click build to pack into a <code>.thor</code> archive with zlib compression and automatically increment <code>plist.txt</code>.",
            step5Title: "Generate Standalone Patcher",
            step5Desc: "Click <strong>GENERATE STANDALONE PATCHER</strong> to merge configs, icons, and themes into a single standalone <code>.exe</code> ready for players!",

            infoSec3Title: "3. Advanced Features & Tips",
            tip1: "<strong>Auto-Scan Game Folder:</strong> Use the green button on the left sidebar to automatically discover clients, setup tools, and GRF archives with one click.",
            tip2: "<strong>Light / Dark Mode:</strong> Toggle themes at the top right for optimal visual comfort (persisted in local storage).",
            tip3: "<strong>Self-Update Engine:</strong> When a patch contains a newer patcher binary, the client updates itself seamlessly without forcing players to re-download.",
            infoDiscordTitle: "4. Discord Community & Support",
            infoDiscordDesc: "Join our official Discord community for support, custom themes, and updates:"
        },

        th: {
            // General & Nav
            appName: "RO Patcher",
            appBadge: "BUILDER",
            navServer: "เซิร์ฟเวอร์ & มิลเลอร์",
            navClient: "ตัวเกม & GRF",
            navTheme: "ธีมหน้าต่าง & ไอคอน",
            navMaker: "สร้างไฟล์แพตช์ (.thor)",
            navGenerate: "สร้างตัว Patcher",
            btnScanGame: "สแกนหาโฟลเดอร์เกม",
            btnResetDefaults: "รีเซ็ตค่าเริ่มต้น",
            btnGuideInfo: "คู่มือ & ข้อมูล",
            langLabel: "ไทย",
            themeDark: "โหมดมืด",
            themeLight: "โหมดสว่าง",

            // Tab Titles & Descs
            tabServerTitle: "การตั้งค่าเซิร์ฟเวอร์และมิลเลอร์",
            tabServerDesc: "กำหนด URL เซิร์ฟเวอร์แพตช์หลัก เซิร์ฟเวอร์สำรอง และหน้าข่าวสารประกาศ",
            tabClientTitle: "การตั้งค่าตัวเกมและ GRF",
            tabClientDesc: "เลือกไฟล์ตัวเกมหลัก พารามิเตอร์รันเกม และตั้งค่าการรวมไฟล์ GRF",
            tabThemeTitle: "ปรับแต่งธีม UI และหน้าต่าง Patcher",
            tabThemeDesc: "เลือกขนาดหน้าต่าง Patcher ธีม HTML/CSS และไฟล์ไอคอน .ico",
            tabMakerTitle: "เครื่องมือสร้างไฟล์แพตช์ (.thor)",
            tabMakerDesc: "บีบอัดไฟล์อัปเดตเป็น .thor พร้อมอัปเดตไฟล์ plist.txt อัตโนมัติ",
            tabGenerateTitle: "สร้างและส่งออก Patcher ตัวเดี่ยว",
            tabGenerateDesc: "ส่งออกไฟล์ .exe ตัวเดี่ยวพร้อมฝังการตั้งค่าและธีม ไม่ต้องมี DLL แยก",

            // Tab 1: Server
            cardPrimaryServer: "เซิร์ฟเวอร์แพตช์หลัก",
            labelServerName: "ชื่อเซิร์ฟเวอร์",
            labelPlistUrl: "URL ไฟล์รายการแพตช์ (plist.txt)",
            labelPatchDir: "URL โฟลเดอร์เก็บไฟล์แพตช์ (.thor)",
            hintPatchDir: "ต้องลงท้ายด้วยเครื่องหมาย / ซึ่งเป็นโฟลเดอร์ที่เก็บไฟล์ .thor",
            labelNoticeUrl: "URL หน้าข่าวสาร / ประกาศ (ไม่บังคับ)",
            cardMirrors: "เซิร์ฟเวอร์สำรอง (Mirrors)",
            btnAddMirror: "+ เพิ่มเซิร์ฟเวอร์สำรอง",
            descMirrors: "หากเซิร์ฟเวอร์หลักไม่ตอบสนอง ระบบ Patcher จะค้นหาและดาวน์โหลดจากเซิร์ฟเวอร์สำรองให้อัตโนมัติ",

            // Tab 2: Client & GRF
            cardClientExec: "ไฟล์ตัวเกมและการเปิดใช้งาน",
            labelClientExe: "ชื่อไฟล์ Executable ของตัวเกม",
            labelSetupExe: "โปรแกรมตั้งค่ากราฟิก (Setup)",
            labelLaunchArgs: "พารามิเตอร์รันเกม (Arguments)",
            hintLaunchArgs: "พารามิเตอร์ที่ตัวเกมหรือ Gepard Shield / Anti-cheat ต้องการ",
            cardGrfSettings: "การตั้งค่าไฟล์ GRF และการรวมไฟล์",
            labelDefaultGrf: "ชื่อไฟล์ GRF หลักเริ่มต้น",
            labelConcurrentDl: "จำนวนสตรีมดาวน์โหลดพร้อมกัน",
            toggleInplace: "รวมไฟล์เข้า GRF โดยตรง (Fast In-Place)",
            descInplace: "เขียนไฟล์ลง GRF เดิมทันทีโดยไม่ต้องสร้างใหม่ทั้งก้อน ประหยัดเวลาและปลอดภัย",
            toggleAutocreate: "สร้างไฟล์ GRF ใหม่อัตโนมัติ",
            descAutocreate: "สร้างไฟล์ GRF v2.0 เปล่าขึ้นมาใหม่อัตโนมัติหากยังไม่มีไฟล์อยู่ในโฟลเดอร์",
            toggleIntegrity: "ตรวจสอบความถูกต้องของไฟล์ (Checksums)",
            descIntegrity: "ตรวจสอบความสมบูรณ์และ Checksum ของไฟล์ .thor ก่อนเริ่มแตกไฟล์",

            // Tab 3: UI Theme
            cardWindowCustom: "ขนาดและลักษณะหน้าต่าง Patcher",
            labelWinTitle: "ชื่อบนแถบหน้าต่าง Patcher",
            labelDimensions: "ขนาดหน้าต่าง (กว้าง × สูง)",
            toggleTransparent: "หน้าต่างโปร่งแสงไร้ขอบ (Frameless Transparent)",
            descTransparent: "เปิดมุมโค้งมนและเอฟเฟกต์ Glassmorphism แบบไร้ขอบ Windows",
            toggleResizable: "อนุญาตให้ปรับขนาดหน้าต่างได้",
            descResizable: "อนุญาตให้ผู้เล่นยืดขยายขนาดหน้าต่าง Patcher ได้",
            labelCustomIcon: "ไฟล์ไอคอนเกมที่ต้องการฝัง (.ico)",
            hintCustomIcon: "จะถูกฝังลงใน PE Binary โดยตรงก่อนรวมไฟล์ข้อมูล",
            cardThemeTemplate: "การเลือกธีมหน้าตาของ Patcher",
            themeCyberTitle: "Modern Cyber Neon (ธีมมาตรฐาน)",
            themeCyberDesc: "สไตล์ Glassmorphism ดีไซน์นีออนเรืองแสง พร้อมหลอดดาวน์โหลดเคลื่อนไหวสมูท",
            themeAnimeTitle: "Sakura Anime Fantasy (ธีมอนิเมะญี่ปุ่น)",
            themeAnimeDesc: "สไตล์อนิเมะแฟนตาซี กลีบซากุระร่วงปลิวสมจริง เอฟเฟกต์เสียงน่ารัก พร้อมภาพมาสคอตและแบนเนอร์สวยงาม",
            labelCustomThemeFolder: "หรือเลือกโฟลเดอร์ธีมแบบกำหนดเอง (HTML/CSS)",
            bgmStatusDetectedTitle: "ตรวจพบเพลงประกอบ Background Music (BGM)",
            bgmStatusDetectedDesc: "พบไฟล์ {path} ({size} MB) — จะฝังลงใน Patcher และเล่นอัตโนมัติพร้อมปุ่มเปิด/ปิดเสียง",
            bgmStatusNoneTitle: "ไม่มีเพลงประกอบ (ไม่บังคับ)",
            bgmStatusNoneDesc: "ไม่พบไฟล์ assets/bgm.mp3 ในธีมนี้ — สามารถ Generate Patcher ได้ตามปกติโดยไม่มีเสียงเพลง",
            bgmStatusMissingTitle: "ไม่พบโฟลเดอร์ธีม หรือไม่มีไฟล์ index.html",
            bgmStatusMissingDesc: "กรุณาตรวจสอบพาธโฟลเดอร์ธีมให้ถูกต้องก่อนทำการ Generate",

            // Tab 4: Patch Maker
            cardCreateThor: "สร้างไฟล์แพตช์ THOR (.thor)",
            descCreateThor: "บีบอัดไฟล์อัปเดตเป็นฟอร์แมต .thor มาตรฐานด้วย Zlib, รองรับชื่อไฟล์เกาหลี CP949 และรายการสั่งลบไฟล์ (Delete List)",
            labelInputDir: "โฟลเดอร์ต้นทาง (ที่บรรจุ data/, System/ หรือไฟล์ที่ต้องการแพตช์)",
            labelInputSingleFile: "เลือกไฟล์เดี่ยวต้นทาง (Single File)",
            labelInputExeFile: "เลือกไฟล์ Patcher หรือตัวเกม (.exe)",
            labelSingleTarget: "พาธปลายทางใน GRF หรือโฟลเดอร์เกม (เว้นว่างเพื่อใช้ชื่อไฟล์เดิม)",
            labelOutputThor: "ที่อยู่และชื่อไฟล์ .thor ปลายทาง",
            labelTargetGrf: "ชื่อไฟล์ GRF เป้าหมาย",
            hintTargetGrf: "เว้นว่างไว้หากต้องการให้แพตช์เข้าสู่ไฟล์ Default GRF ของ Patcher อัตโนมัติ",
            toggleMergeGrf: "โหมด RO - GRF (รวมไฟล์เข้าสู่ไฟล์ GRF)",
            descMergeGrf: "หากไม่ติ๊ก จะเป็นโหมด File แตกไฟล์ลงโฟลเดอร์เกมโดยตรง (เช่น System/*.lub, .exe, .dll)",
            labelPlistPath: "อัปเดตไฟล์ plist.txt อัตโนมัติ (ไม่บังคับ)",
            subtabAddFile: "เพิ่มไฟล์ (Add File)",
            subtabDeleteFile: "ลบไฟล์ (Delete File)",
            legendInputMode: "รูปแบบไฟล์ต้นทาง (Input Mode)",
            radioModeDir: "ทั้งโฟลเดอร์ (Directory)",
            radioModeSingle: "ไฟล์เดี่ยว (Single File)",
            radioModeExe: "ไฟล์ตัวเกม / Patcher (.EXE)",
            legendCompLevel: "ระดับการบีบอัดข้อมูล (Compression Level)",
            radioCompLow: "ต่ำ (เร็ว)",
            radioCompMed: "ปานกลาง",
            radioCompHigh: "สูง (บีบอัดสูงสุด)",
            legendEncoding: "การเข้ารหัสชื่อไฟล์ (Filename Encoding)",
            radioEncUnicode: "Unicode (รองรับภาษาเกาหลี CP949)",
            radioEncAscii: "ASCII (รหัสเก่า Win-1252)",
            labelDeleteList: "รายการไฟล์ที่จะถูกลบออกจาก GRF หรือโฟลเดอร์เกม",
            btnImportTxt: "นำเข้าจาก .txt",
            btnImportClipboard: "วางจากคลิปบอร์ด",
            btnClearDelete: "ล้างรายการ",
            labelAddDelete: "เพิ่มพาธไฟล์ที่ต้องการลบ (Add to list)",
            btnAddDelete: "เพิ่ม!",
            emptyDeleteList: "ยังไม่มีรายการไฟล์ที่สั่งลบ สามารถพิมพ์เพิ่มด้านล่าง หรือนำเข้าจากไฟล์ .txt / คลิปบอร์ด",
            btnBuildThor: "สร้างไฟล์แพตช์ .THOR ทันที",

            // Tab 5: Generate
            heroGenerateTitle: "สร้างไฟล์ Patcher แบบ Standalone ตัวเดี่ยว",
            heroGenerateDesc: "รวมการตั้งค่าเซิร์ฟเวอร์ ธีม UI และแอสเซททั้งหมดเข้าเป็นไฟล์ .exe ตัวเดี่ยวพร้อมแจกจ่ายให้ผู้เล่น ไม่ต้องมีโฟลเดอร์หรือไฟล์ config เสริม",
            labelGenOutput: "ที่อยู่และชื่อไฟล์ Executable ปลายทาง (.exe)",
            labelGenTemplate: "ไฟล์ Base Template Binary (patcher_base.exe)",
            btnGeneratePatcher: "สร้างโปรแกรม PATCHER ตัวเดี่ยว (.EXE) ทันที",

            // Scan Modal
            scanModalTitle: "สแกนหาโฟลเดอร์เกมอัตโนมัติ",
            scanModalPrompt: "ระบุพาธของโฟลเดอร์เกม Ragnarok Online:",
            btnCancelScan: "ยกเลิก",
            btnDoScan: "เริ่มสแกนโฟลเดอร์",

            // Info Modal
            infoBadge: "คู่มือและรายละเอียดการใช้งาน",
            infoTitle: "คู่มือและรายละเอียดฟังก์ชัน Modern RO Patcher Builder",
            infoCloseBtn: "เข้าใจแล้ว (Close)",
            infoSec1Title: "1. ภาพรวมของระบบ (System Overview)",
            infoSec1Text: "<strong>Modern RO Patcher Builder</strong> คือเครื่องมือสร้างและจัดการโปรแกรม Client Patcher ยุคใหม่สำหรับ Ragnarok Online พัฒนาด้วย <strong>Rust 2021 + Microsoft Edge WebView2</strong>",
            infoTag1: "✔ ไม่ติด Anti-Virus (No False Positives)",
            infoTag2: "✔ Single Executable (ไม่มี DLL ภายนอก)",
            infoTag3: "✔ Multi-stream Download สูงสุด 32 เธรด",
            infoTag4: "✔ In-Place GRF Merging รวดเร็ว ปลอดภัย",
            infoTag5: "✔ Binary Overlay Engine (ROPATCH1)",
            
            infoSec2Title: "2. ขั้นตอนการใช้งานทีละ Step",
            step1Title: "Server & Mirrors (ตั้งค่าเซิร์ฟเวอร์แพตช์)",
            step1Desc: "ระบุ URL ของไฟล์รายการแพตช์ <code>plist.txt</code> และโฟลเดอร์เก็บไฟล์ <code>.thor</code> สามารถเพิ่ม <strong>Mirror Servers</strong> ได้ไม่จำกัด หากเซิร์ฟเวอร์หลักล่ม Patcher จะสลับไปโหลดจาก Mirror อัตโนมัติ",
            step2Title: "Game & GRF (ตั้งค่าตัวเกมและไฟล์ GRF)",
            step2Desc: "ระบุชื่อไฟล์เกม (เช่น <code>_PRONTERA BOX.exe</code>), Arguments (เช่น <code>-1rag1</code>), ตัวตั้งค่ากราฟิก (เช่น <code>_PRONTERA BOX - Settings.exe</code>) และชื่อไฟล์ GRF หลัก พร้อมตั้งค่าความเร็วเธรดดาวน์โหลด (1–32 streams)",
            step3Title: "UI Theme & Window (ปรับแต่งหน้าตา Patcher)",
            step3Desc: "เลือกขนาดหน้าต่าง (กว้าง x สูง), ความโปร่งแสง (Transparent Frameless), เลือกไฟล์ไอคอนเกม <code>.ico</code> และเลือกโฟลเดอร์ธีม Web UI (HTML/CSS) ตามต้องการ",
            step4Title: "Patch Maker (เครื่องมือสร้างไฟล์แพตช์ .thor)",
            step4Desc: "นำไฟล์เกมใหม่ ๆ เช่น <code>data/</code> วางในโฟลเดอร์ต้นทาง จากนั้นกดสร้างไฟล์ <code>.thor</code> พร้อมตั้งค่าให้เขียนทับไฟล์ <code>plist.txt</code> รันเลข Patch Index ให้อัตโนมัติ",
            step5Title: "Generate Patcher (ส่งออกไฟล์ Patcher ตัวจริง)",
            step5Desc: "กดปุ่ม <strong>GENERATE STANDALONE PATCHER</strong> ระบบจะนำ Config, Icon และ UI Theme ผสานเป็นไฟล์ <code>.exe</code> ตัวเดียวพร้อมแจกจ่ายให้ผู้เล่นได้ทันที!",

            infoSec3Title: "3. ฟังก์ชันและเทคนิคพิเศษ",
            tip1: "<strong>Auto-Scan Game Folder:</strong> ปุ่มลัดสีเขียวที่แถบเมนูด้านซ้าย ช่วยสแกนค้นหา Executable, Settings, และไฟล์ GRF ในโฟลเดอร์เกมให้อัตโนมัติในคลิกเดียว",
            tip2: "<strong>Light / Dark Mode:</strong> กดปุ่มสลับธีมที่มุมบนขวา เพื่อเลือกโทนสีตามความสบายตา (บันทึกจำค่าอัตโนมัติ)",
            tip3: "<strong>Self-Update Support:</strong> เมื่อสร้างแพตช์ที่มีไฟล์ Patcher ตัวใหม่ ตัว Patcher ของผู้เล่นจะอัปเดตตัวเองอัตโนมัติโดยไม่ต้องให้ผู้เล่นดาวน์โหลดใหม่",
            infoDiscordTitle: "4. กลุ่มคอมมูนิตี้ Discord และสอบถามข้อมูล",
            infoDiscordDesc: "เข้าร่วมกลุ่ม Discord เพื่อพูดคุย แลกเปลี่ยนธีม สอบถามเทคนิค หรือแจ้งปัญหา:"
        }
    },

    t: function(key) {
        const langDict = this.translations[this.currentLang] || this.translations.en;
        return langDict[key] || this.translations.en[key] || key;
    },

    setLanguage: function(lang) {
        if (!this.translations[lang]) lang = "en";
        this.currentLang = lang;
        try {
            localStorage.setItem("app-lang", lang);
        } catch (e) {}

        document.documentElement.lang = lang;

        // 1. Update text nodes with data-i18n
        document.querySelectorAll("[data-i18n]").forEach(el => {
            const key = el.getAttribute("data-i18n");
            const val = this.t(key);
            if (val) {
                el.innerHTML = val;
            }
        });

        // 2. Update placeholder attributes with data-i18n-placeholder
        document.querySelectorAll("[data-i18n-placeholder]").forEach(el => {
            const key = el.getAttribute("data-i18n-placeholder");
            const val = this.t(key);
            if (val) {
                el.setAttribute("placeholder", val);
            }
        });

        // 3. Update Language button
        const langFlag = document.getElementById("langFlag");
        const langLabel = document.getElementById("langLabel");
        if (langFlag && langLabel) {
            langFlag.innerHTML = "&#127760;";
            if (lang === "th") {
                langLabel.innerText = "ไทย (TH)";
            } else {
                langLabel.innerText = "English (EN)";
            }
        }

        // 4. Update Theme button text
        const themeLabel = document.getElementById("themeLabel");
        const currentTheme = document.documentElement.getAttribute("data-theme") || "dark";
        if (themeLabel) {
            themeLabel.innerText = currentTheme === "light" ? this.t("themeLight") : this.t("themeDark");
        }

        // 5. Update active tab title and description
        const activeNav = document.querySelector(".nav-item.active");
        if (activeNav) {
            const tabId = activeNav.getAttribute("data-tab");
            const pageTitle = document.getElementById("pageTitle");
            const pageDesc = document.getElementById("pageDesc");
            if (pageTitle && pageDesc) {
                switch (tabId) {
                    case "tab-server":
                        pageTitle.innerText = this.t("tabServerTitle");
                        pageDesc.innerText = this.t("tabServerDesc");
                        break;
                    case "tab-client":
                        pageTitle.innerText = this.t("tabClientTitle");
                        pageDesc.innerText = this.t("tabClientDesc");
                        break;
                    case "tab-theme":
                        pageTitle.innerText = this.t("tabThemeTitle");
                        pageDesc.innerText = this.t("tabThemeDesc");
                        break;
                    case "tab-maker":
                        pageTitle.innerText = this.t("tabMakerTitle");
                        pageDesc.innerText = this.t("tabMakerDesc");
                        break;
                    case "tab-generate":
                        pageTitle.innerText = this.t("tabGenerateTitle");
                        pageDesc.innerText = this.t("tabGenerateDesc");
                        break;
                }
            }
        }
    },

    init: function() {
        let savedLang = "en";
        try {
            savedLang = localStorage.getItem("app-lang") || (navigator.language && navigator.language.startsWith("th") ? "th" : "en");
        } catch (e) {}
        this.setLanguage(savedLang);
    }
};

window.I18N = I18N;
