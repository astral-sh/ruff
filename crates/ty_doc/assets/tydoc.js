(() => {
    let indexPromise = null;
    const KIND = 0;
    const NAME = 1;
    const PATH = 2;
    const HREF = 3;
    const SUMMARY = 4;

    function normalize(value) {
        return value.toLowerCase();
    }

    function loadSearchIndex(root) {
        if (window.tyDocSearchIndex) {
            return Promise.resolve(window.tyDocSearchIndex);
        }

        if (indexPromise) {
            return indexPromise;
        }

        indexPromise = new Promise((resolve, reject) => {
            const script = document.createElement("script");
            script.src = root + "search-index.js";
            script.onload = () => resolve(window.tyDocSearchIndex || []);
            script.onerror = () => reject(new Error("failed to load search index"));
            document.head.append(script);
        });

        return indexPromise;
    }

    function score(item, query) {
        const name = normalize(item[NAME]);
        const path = normalize(item[PATH]);
        const kind = normalize(item[KIND]);
        if (name === query || path === query) {
            return 0;
        }
        if (name.startsWith(query)) {
            return 1;
        }
        if (path.startsWith(query)) {
            return 2;
        }
        if (name.includes(query)) {
            return 3;
        }
        if (path.includes(query)) {
            return 4;
        }
        if (kind.includes(query) || normalize(item[SUMMARY]).includes(query)) {
            return 5;
        }
        return null;
    }

    function appendResult(results, root, item) {
        const link = document.createElement("a");
        link.className = "search-result";
        link.href = root + item[HREF];

        const header = document.createElement("span");
        header.className = "search-result-title";

        const badge = document.createElement("span");
        badge.className = "search-kind";
        badge.textContent = item[KIND];

        const name = document.createElement("span");
        name.textContent = item[NAME];

        header.append(badge, name);
        link.append(header);

        if (item[PATH] && item[PATH] !== item[NAME]) {
            const path = document.createElement("span");
            path.className = "search-result-path";
            path.textContent = item[PATH];
            link.append(path);
        }

        if (item[SUMMARY]) {
            const summary = document.createElement("span");
            summary.className = "search-summary";
            summary.textContent = item[SUMMARY];
            link.append(summary);
        }

        results.append(link);
    }

    function initSearch() {
        const input = document.getElementById("tydoc-search");
        const results = document.getElementById("tydoc-search-results");
        if (!input || !results) {
            return;
        }

        const root = document.body.dataset.tydocRoot || "";

        let renderVersion = 0;

        async function render() {
            const version = ++renderVersion;
            const query = normalize(input.value.trim());
            results.replaceChildren();

            if (!query) {
                results.hidden = true;
                return;
            }

            const loading = document.createElement("div");
            loading.className = "search-empty";
            loading.textContent = "Loading search index...";
            results.append(loading);
            results.hidden = false;

            let index;
            try {
                index = await loadSearchIndex(root);
            } catch {
                if (version === renderVersion) {
                    loading.textContent = "Search index failed to load";
                }
                return;
            }

            if (version !== renderVersion) {
                return;
            }

            results.replaceChildren();

            const matches = index
                .map((item) => ({ item, score: score(item, query) }))
                .filter((match) => match.score !== null)
                .sort((left, right) => left.score - right.score || left.item[PATH].localeCompare(right.item[PATH]))
                .slice(0, 20);

            if (matches.length === 0) {
                const empty = document.createElement("div");
                empty.className = "search-empty";
                empty.textContent = "No results";
                results.append(empty);
            } else {
                for (const match of matches) {
                    appendResult(results, root, match.item);
                }
            }

            results.hidden = false;
        }

        input.addEventListener("input", render);
        input.addEventListener("keydown", (event) => {
            if (event.key === "Escape") {
                input.value = "";
                results.replaceChildren();
                results.hidden = true;
            } else if (event.key === "Enter") {
                const first = results.querySelector("a");
                if (first) {
                    event.preventDefault();
                    first.click();
                }
            }
        });
    }

    function initDetailsToggles() {
        for (const details of document.querySelectorAll(".col")) {
            const toggle = details.querySelector(
                ":scope > .isum > .tog, :scope > .sum > .tog",
            );
            const content = details.querySelector(":scope > .dc");
            if (!toggle || !content) {
                continue;
            }

            function syncExpanded() {
                const expanded = details.classList.contains("open");
                toggle.setAttribute("aria-expanded", expanded ? "true" : "false");
                content.hidden = !expanded;
            }

            toggle.addEventListener("click", () => {
                details.classList.toggle("open");
                syncExpanded();
            });
            syncExpanded();
        }
    }

    document.addEventListener("DOMContentLoaded", () => {
        initSearch();
        initDetailsToggles();
    });
})();
