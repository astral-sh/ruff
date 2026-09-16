(() => {
  let restoreFilters = () => {};
  let layoutObserver;
  let filterListeners;
  const collator = new Intl.Collator(undefined, { numeric: true });

  document$.subscribe(() => {
    layoutObserver?.disconnect();
    filterListeners?.abort();
    restoreFilters = () => {};

    const form = document.getElementById("rule-filters");
    if (!form) {
      return;
    }

    const article = form.closest("article");
    const table = article.querySelector(".rule-category")?.closest("table");
    if (!table) {
      return;
    }

    table.id = "rule-catalog";
    const categoryMetadata =
      table.tHead.querySelector(".rule-category").dataset;
    const categoryOrder = categoryMetadata.categories.split(" ");
    const body = table.tBodies[0];

    const rows = [...body.rows].map((element) => ({
      element,
      code: element.querySelector(".rule-code").textContent.trim(),
      name: element.querySelector(".rule-identity code").textContent.trim(),
      text: [".rule-code", ".rule-identity"]
        .map((selector) => element.querySelector(selector).textContent)
        .join(" ")
        .toLowerCase(),
      category: element.querySelector(".rule-category").textContent.trim(),
      linter: element.querySelector(".rule-linter").dataset.linter,
      linterLabel: element.querySelector(".rule-linter").dataset.linterLabel,
      status: element.querySelector(".rule-status").dataset.status,
      fixable: element.querySelector(".rule-status").dataset.fixable === "true",
      isDefault:
        element.querySelector(".rule-status").dataset.default === "true",
    }));

    const linters = new Set(rows.map((row) => row.linter));
    const search = form.elements.namedItem("rule-search");
    const count = form.querySelector(".rule-filters__count");
    const empty = form.querySelector(".rule-filters__empty");

    const filters = [...form.querySelectorAll("[data-filter]")].map(
      (details) => {
        const name = details.dataset.filter;
        const summary = details.querySelector("summary");
        const selection = summary.firstElementChild;
        const options = [...details.querySelectorAll("input")];
        const allValues = options.map((option) => option.value);

        const isAll = () => options.every((option) => option.checked);
        const values = () =>
          options
            .filter((option) => option.checked)
            .map((option) => option.value);

        const updateSummary = () => {
          const labels = options
            .filter((option) => option.checked)
            .map((option) => option.parentElement.textContent.trim());

          selection.textContent =
            labels.length === options.length
              ? "All"
              : labels.length === 0
                ? "None"
                : labels.length === 1
                  ? labels[0]
                  : `${labels.length} selected`;
          summary.title =
            isAll() || !labels.length
              ? selection.textContent
              : labels.join(", ");
          summary.setAttribute(
            "aria-label",
            `${details.closest("fieldset").querySelector("legend").textContent}: ${selection.textContent}`,
          );
        };

        const setValues = (selected = allValues) => {
          for (const option of options) {
            option.checked = selected.includes(option.value);
          }
        };

        const presets = {
          all: allValues,
          default: categoryMetadata.defaultCategories.split(" "),
          none: [],
        };

        for (const button of details.querySelectorAll("[data-preset]")) {
          button.addEventListener("click", () => {
            setValues(presets[button.dataset.preset]);
            applyFilters();
          });
        }

        details.addEventListener("toggle", () => {
          if (details.open) {
            for (const other of filters) {
              if (other.details !== details) {
                other.details.open = false;
              }
            }
          }
        });

        details.addEventListener("focusout", (event) => {
          if (event.relatedTarget && !details.contains(event.relatedTarget)) {
            details.open = false;
          }
        });

        return {
          name,
          details,
          summary,
          values,
          isAll,
          setValues,
          updateSummary,
        };
      },
    );
    const [category, linter, status] = filters;

    filterListeners = new AbortController();
    document.addEventListener(
      "click",
      (event) => {
        for (const { details } of filters) {
          if (!details.contains(event.target)) {
            details.open = false;
          }
        }
      },
      { signal: filterListeners.signal },
    );

    document.addEventListener(
      "keydown",
      (event) => {
        if (event.key !== "Escape") {
          return;
        }

        for (const { details, summary } of filters) {
          if (details.open) {
            details.open = false;
            summary.focus();
            event.preventDefault();
          }
        }
      },
      { signal: filterListeners.signal },
    );

    function clearFilters() {
      search.value = "";
      for (const filter of filters) {
        filter.setValues();
      }
    }

    const container = table.closest(".md-typeset__scrollwrap") || table;
    container.classList.add("rule-catalog-scroll");
    container.setAttribute("role", "region");
    container.setAttribute("aria-label", "Rules");
    container.tabIndex = 0;
    form.hidden = false;

    for (const toc of document.querySelectorAll(".md-nav--secondary")) {
      toc.hidden = true;
      toc.parentElement.classList.add("rule-index-nav");
    }

    const columns = ["code", "name", "category", "linter"];
    let sortBy = "linter";
    let descending = false;
    const headers = [...table.tHead.rows[0].cells];

    for (const [index, key] of columns.entries()) {
      const button = headers[index].querySelector("button");
      button.disabled = false;
      button.addEventListener("click", () => {
        descending = sortBy === key ? !descending : key === "category";
        sortBy = key;
        sortRows();
        updateURL();
      });
    }

    const siteHeader = document.querySelector(".md-header");
    layoutObserver = new ResizeObserver(() => {
      article.style.setProperty(
        "--rule-site-header-height",
        `${siteHeader.offsetHeight}px`,
      );
      article.style.setProperty(
        "--rule-filters-height",
        `${form.offsetHeight}px`,
      );
      article.style.setProperty(
        "--rule-columns-height",
        `${table.tHead.offsetHeight}px`,
      );
    });

    for (const element of [siteHeader, form, table.tHead]) {
      layoutObserver.observe(element);
    }

    function sortRows() {
      for (const [index, key] of columns.entries()) {
        headers[index].setAttribute(
          "aria-sort",
          key === sortBy ? (descending ? "descending" : "ascending") : "none",
        );
      }

      const sorted = [...rows].sort((a, b) => {
        // Keep rules without codes or an originating linter at the end.
        if (
          (sortBy === "code" || sortBy === "linter") &&
          (a.code === "—" || b.code === "—")
        ) {
          return Number(a.code === "—") - Number(b.code === "—");
        }

        const key = sortBy === "linter" ? "linterLabel" : sortBy;

        // Categories are generated from highest to lowest severity.
        const comparison =
          sortBy === "category"
            ? categoryOrder.indexOf(b.category) -
              categoryOrder.indexOf(a.category)
            : collator.compare(a[key], b[key]);

        return (
          (descending ? -1 : 1) * comparison ||
          (sortBy === "linter" ? collator.compare(a.code, b.code) : 0) ||
          collator.compare(a.name, b.name)
        );
      });
      body.append(...sorted.map((row) => row.element));
    }

    function filter() {
      for (const filter of filters) {
        filter.updateSummary();
      }

      const categories = category.values();
      const selectedLinters = linter.values();
      const statuses = status.values();
      const terms = search.value
        .trim()
        .toLowerCase()
        .split(/\s+/)
        .filter(Boolean);

      let visible = 0;
      for (const row of rows) {
        const matches =
          categories.includes(row.category) &&
          selectedLinters.includes(row.linter) &&
          statuses.some(
            (value) =>
              value === row.status ||
              (value === "fixable" && row.fixable) ||
              (value === "default" && row.isDefault),
          ) &&
          terms.every((term) => row.text.includes(term));

        row.element.hidden = !matches;
        if (matches) {
          visible++;
        }
      }

      count.textContent =
        visible === rows.length
          ? `${visible} rules`
          : `${visible} of ${rows.length} rules`;
      empty.hidden = visible !== 0;
      container.hidden = visible === 0;
    }

    const fragmentLinks = [...article.querySelectorAll("a[href]")].filter(
      (link) =>
        link.hash &&
        link.origin === location.origin &&
        link.pathname === location.pathname,
    );

    function updateFragmentLinks() {
      // Material resolves fragment links before filters can change the query.
      for (const link of fragmentLinks) {
        link.search = location.search;
      }
    }

    function updateURL(clearFragment = false) {
      const url = new URL(location.href);
      if (clearFragment) {
        url.hash = "";
      }

      if (search.value) {
        url.searchParams.set(search.name, search.value);
      } else {
        url.searchParams.delete(search.name);
      }

      for (const filter of filters) {
        url.searchParams.delete(filter.name);
        if (filter.isAll()) {
          continue;
        }

        const selected = filter.values();
        // An omitted parameter means All; preserve an explicit empty selection.
        if (!selected.length) {
          url.searchParams.set(filter.name, "none");
        }

        for (const value of selected) {
          url.searchParams.append(filter.name, value);
        }
      }

      if (sortBy !== "linter") {
        url.searchParams.set("sort", sortBy);
      } else {
        url.searchParams.delete("sort");
      }

      if (descending) {
        url.searchParams.set("dir", "desc");
      } else {
        url.searchParams.delete("dir");
      }

      history.replaceState(history.state, "", url);
      updateFragmentLinks();
      window.dispatchEvent(new Event("rule-filter-change"));
    }

    restoreFilters = () => {
      if (!form.isConnected) {
        return;
      }

      const url = new URL(location.href);
      search.value = url.searchParams.get(search.name) || "";
      for (const filter of filters) {
        filter.setValues(
          url.searchParams.has(filter.name)
            ? url.searchParams.getAll(filter.name)
            : undefined,
        );
      }

      sortBy = columns.includes(url.searchParams.get("sort"))
        ? url.searchParams.get("sort")
        : "linter";
      descending = url.searchParams.get("dir") === "desc";

      let fragment;
      try {
        fragment = decodeURIComponent(url.hash.slice(1));
      } catch {
        fragment = "";
      }

      // Former linter heading links now open the corresponding filtered view.
      if (linters.has(fragment)) {
        clearFilters();
        linter.setValues([fragment]);
        updateURL();
      }

      filter();
      sortRows();
      updateFragmentLinks();

      const target = fragment && document.getElementById(fragment);
      if (!target || !article.contains(target)) {
        return;
      }

      const details = target.closest("details");
      if (details) {
        details.open = true;
      }

      if (target.closest("[hidden]")) {
        clearFilters();
        filter();
        updateURL();
        target.scrollIntoView();
      }
    };

    function applyFilters() {
      filter();
      updateURL(true);
    }

    form.addEventListener("input", applyFilters);

    form.addEventListener("submit", (event) => {
      event.preventDefault();
      applyFilters();
    });

    form.addEventListener("reset", (event) => {
      event.preventDefault();
      clearFilters();
      applyFilters();
    });

    restoreFilters();
  });

  location$.subscribe(() => restoreFilters());
})();
