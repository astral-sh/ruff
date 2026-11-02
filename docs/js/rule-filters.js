(() => {
  let restoreFilters = () => {};

  document$.subscribe(() => {
    restoreFilters = () => {};

    const form = document.getElementById("rule-filters");
    if (!form || !("popover" in HTMLElement.prototype)) {
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
    const body = table.tBodies[0];

    const rows = Array.from(body.rows, (element) => {
      const [code, identity, category, linter, status] = element.cells;
      return {
        element,
        text: `${code.textContent} ${identity.querySelector("code").textContent}`.toLowerCase(),
        category: category.textContent.trim(),
        linter: linter.dataset.linter,
        status: status.dataset.status,
      };
    });

    const search = form.elements.namedItem("rule-search");
    const count = form.querySelector(".rule-filters__count");
    const empty = form.querySelector(".rule-filters__empty");

    const filters = [...form.querySelectorAll("[data-filter]")].map((group) => {
      const name = group.dataset.filter;
      const trigger = group.querySelector("[popovertarget]");
      const panel = group.querySelector("[popover]");
      const selection = trigger.firstElementChild;
      const options = [...panel.querySelectorAll("input")];
      const allValues = options.map((option) => option.value);

      const isAll = () => options.every((option) => option.checked);

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
        trigger.title =
          isAll() || !labels.length ? selection.textContent : labels.join(", ");
        trigger.setAttribute(
          "aria-label",
          `${group.closest("fieldset").querySelector("legend").textContent}: ${selection.textContent}`,
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

      for (const button of panel.querySelectorAll("[data-preset]")) {
        button.addEventListener("click", () => {
          setValues(presets[button.dataset.preset]);
          applyFilters();
        });
      }

      const controls = [...panel.querySelectorAll("button, input")];
      group.addEventListener("keydown", (event) => {
        if (
          !panel.matches(":popover-open") ||
          (event.key !== "ArrowDown" && event.key !== "ArrowUp") ||
          event.altKey ||
          event.ctrlKey ||
          event.metaKey
        ) {
          return;
        }

        event.preventDefault();
        const direction = event.key === "ArrowDown" ? 1 : -1;
        const index = controls.indexOf(event.target);
        const nextIndex =
          index === -1
            ? direction === 1
              ? 0
              : controls.length - 1
            : Math.max(0, Math.min(controls.length - 1, index + direction));
        controls[nextIndex]?.focus();
      });

      group.addEventListener("focusout", (event) => {
        if (
          panel.matches(":popover-open") &&
          event.relatedTarget &&
          !group.contains(event.relatedTarget)
        ) {
          panel.hidePopover();
        }
      });

      return {
        name,
        isAll,
        setValues,
        updateSummary,
      };
    });

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

    function filter() {
      for (const filter of filters) {
        filter.updateSummary();
      }

      const formData = new FormData(form);
      const categories = formData.getAll("category");
      const selectedLinters = formData.getAll("linter");
      const statuses = formData.getAll("status");
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
          statuses.includes(row.status) &&
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

    const fragmentLinks = [...document.querySelectorAll("a[href]")].filter(
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
      const formData = new FormData(form);
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

        const selected = formData.getAll(filter.name);
        // An omitted parameter means All; preserve an explicit empty selection.
        if (!selected.length) {
          url.searchParams.set(filter.name, "none");
        }

        for (const value of selected) {
          url.searchParams.append(filter.name, value);
        }
      }

      history.replaceState(history.state, "", url);
      updateFragmentLinks();
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

      let fragment;
      try {
        fragment = decodeURIComponent(url.hash.slice(1));
      } catch {
        fragment = "";
      }

      filter();
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
