try {
    const saved = localStorage.getItem("color-scheme") || "dark";
    const meta = document.querySelector('meta[name="color-scheme"]');
    if (meta) {
        meta.content = saved;
    }
    document.documentElement.setAttribute("data-theme", saved);
} catch (e) {}
