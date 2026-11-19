/* Read-only list projections: native contacts and chart, plus the company book's parallel
   lists. Nothing here posts to the ledger, writes back to QuickBooks or runs payroll. */
(() => {
  "use strict";
  const list = value => Array.isArray(value) ? value : [];
  const text = value => String(value ?? "");
  const PARALLEL_BOUNDARY = "Parallel list: kept inside NXT, never synced to QuickBooks, never posted to the ledger.";
  const KIND = { asset: "Asset", liability: "Liability", equity: "Equity", income: "Income", expense: "Expense" };
  // Only the company book's parallel lists accept new entries, and only these three.
  const ADDABLE = { employees: "employees", departments: "departments", items: "items", budgets: "budgets", classes: "classes" };
  // Non-posting documents: estimates and purchase orders. One line is created per request.
  const DOCUMENTS = { estimates: "estimates", "purchase-orders": "purchase_orders" };
  const canAdd = lane => lane?.kind === "parallel" && (!!ADDABLE[lane.id] || !!DOCUMENTS[lane.id] || lane.id === "time");

  function requestId(kind) {
    const random = globalThis.crypto && typeof globalThis.crypto.randomUUID === "function"
      ? globalThis.crypto.randomUUID()
      : Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 10);
    return kind + "-" + random;
  }

  /** Mirrors the server's own limit: a trimmed, non-empty name of at most 80 characters. */
  function nameError(name) {
    const value = text(name).trim();
    if (!value) return "Enter a name.";
    if (value.length > 80) return "Names are limited to 80 characters.";
    if (/[\u0000-\u0008\u000B\u000C\u000E-\u001F]/.test(value)) return "Names cannot contain control characters.";
    return "";
  }

  /** Builds the exact request body the company-books endpoint expects (deny_unknown_fields). */
  function createRequest(laneId, input = {}) {
    const kind = ADDABLE[laneId];
    if (!kind) throw Error("This list does not accept new entries.");
    const name = text(input.name).trim();
    const problem = nameError(name);
    if (problem) throw Error(problem);
    const body = { request_id: text(input.requestId) || requestId(kind), name };
    if (kind === "items") {
      const account = text(input.incomeAccount).trim();
      if (!account) throw Error("Choose an income account.");
      body.income_account = account;
    }
    return { kind, body };
  }

  /** Estimates and purchase orders are non-posting documents: contact, optional number, one line. */
  function createDocumentRequest(laneId, input = {}) {
    const kind = DOCUMENTS[laneId];
    if (!kind) throw Error("This list does not accept non-posting documents.");
    const contact = text(input.contactId).trim();
    if (!contact) throw Error("Choose a contact.");
    const documentNumber = text(input.documentNumber).trim();
    if (documentNumber.length > 40 || documentNumber.includes("/")) throw Error("Document number cannot contain a slash or exceed 40 characters.");
    const itemId = text(input.itemId).trim();
    if (!itemId) throw Error("Choose an item.");
    const cents = Number(input.amountCents);
    if (!Number.isInteger(cents) || cents <= 0) throw Error("Each line needs a positive amount, in whole cents.");
    return {
      kind,
      body: {
        request_id: text(input.requestId) || requestId(kind),
        contact,
        document_number: documentNumber,
        lines: [{ item_id: itemId, amount_cents: cents }]
      }
    };
  }

  /** Applies a payment to one open document. Posts a journal, so every field is checked first. */
  function createPaymentRequest(input = {}) {
    const documentId = text(input.documentId).trim();
    if (!documentId) throw Error("Choose an open document.");
    const expectedHead = text(input.expectedHead).trim();
    if (!expectedHead) throw Error("This book has no committed head; refresh and try again.");
    const operator = text(input.operatorLabel).trim();
    if (!operator) throw Error("Enter your name as the operator.");
    if (operator.length > 120) throw Error("Operator name is too long.");
    const cash = text(input.cashAccountId).trim();
    if (!cash) throw Error("Choose a cash account.");
    const date = text(input.date).trim();
    if (!/^\d{4}-\d{2}-\d{2}$/.test(date) || Number.isNaN(Date.parse(date + "T00:00:00Z"))) throw Error("Enter a date as YYYY-MM-DD.");
    const cents = Number(input.amountCents);
    const open = Number(input.openCents);
    if (!Number.isInteger(cents) || cents <= 0) throw Error("Payment must be positive cents.");
    if (Number.isFinite(open) && cents > open) throw Error("Payment exceeds the open balance.");
    return {
      kind: "payments",
      body: {
        request_id: text(input.requestId) || requestId("payment"),
        expected_head: expectedHead,
        operator_label: operator,
        document_id: documentId,
        cash_account: cash,
        date,
        amount_cents: cents
      }
    };
  }

  /** Integer cents as a plain amount, so exports and screens agree. */
  function amount(cents) { return window.NXTLedgerFormat ? window.NXTLedgerFormat.cents(cents) : ""; }

  /** Budgets take an account, a YYYY-MM period and a non-negative amount. */
  function createBudgetRequest(input = {}) {
    const accountId = text(input.accountId).trim();
    if (!accountId) throw Error("Choose an account.");
    const period = text(input.period).trim();
    if (!/^\d{4}-\d{2}$/.test(period) || !/^(0[1-9]|1[0-2])$/.test(period.slice(5))) throw Error("Enter a period as YYYY-MM.");
    const cents = Number(input.amountCents);
    if (!Number.isInteger(cents) || cents < 0) throw Error("Amount must be zero or more, in whole cents.");
    return { kind: "budgets", body: { request_id: text(input.requestId) || requestId("budget"), account_id: accountId, period, amount_cents: cents } };
  }

  /** Time activities take employee, date and minutes rather than a name. */
  function createTimeRequest(input = {}) {
    const employeeId = text(input.employeeId).trim();
    if (!employeeId) throw Error("Choose an employee.");
    const date = text(input.date).trim();
    if (!/^\d{4}-\d{2}-\d{2}$/.test(date) || Number.isNaN(Date.parse(date + "T00:00:00Z"))) throw Error("Enter a date as YYYY-MM-DD.");
    const minutes = Number(input.minutes);
    if (!Number.isInteger(minutes) || minutes < 1 || minutes > 1440) throw Error("Minutes must be a whole number between 1 and 1440.");
    return { kind: "time", body: { request_id: text(input.requestId) || requestId("time"), employee_id: employeeId, date, minutes } };
  }

  const columns = {
    customer: [
      { key: "name", label: "Name" },
      { key: "email", label: "Email" },
      { key: "phone", label: "Phone" },
      { key: "terms_days", label: "Terms (days)", num: true },
      { key: "archived", label: "Archived", yesno: true }
    ],
    employee: [
      { key: "name", label: "Name" },
      { key: "active", label: "Active", yesno: true }
    ],
    department: [{ key: "name", label: "Name" }],
    item: [
      { key: "name", label: "Name" },
      { key: "income_account", label: "Income account" }
    ],
    budget: [
      { key: "account", label: "Account" },
      { key: "period", label: "Period" },
      { key: "amount", label: "Amount", num: true }
    ],
    subledger: [
      { key: "kind", label: "Kind" },
      { key: "approval", label: "Approval" },
      { key: "contact", label: "Contact" },
      { key: "total", label: "Total", num: true },
      { key: "open", label: "Open", num: true }
    ],
    nonPosting: [
      { key: "contact", label: "Contact" },
      { key: "document", label: "Number" },
      { key: "lines", label: "Lines", num: true },
      { key: "total", label: "Total", num: true }
    ],
    time: [
      { key: "employee", label: "Employee" },
      { key: "date", label: "Date" },
      { key: "minutes", label: "Minutes", num: true }
    ],
    account: [
      { key: "code", label: "Number" },
      { key: "name", label: "Name" },
      { key: "kind", label: "Type" },
      { key: "archived", label: "Archived", yesno: true }
    ]
  };

  function contactsOf(contacts, kind) {
    return list(contacts)
      .filter(row => row && row.kind === kind)
      .map(row => ({
        name: text(row.name),
        email: text(row.email),
        phone: text(row.phone),
        terms_days: Number.isFinite(row.terms_days) ? row.terms_days : "",
        archived: row.archived === true
      }))
      .sort((a, b) => a.name.localeCompare(b.name));
  }

  /** Builds every lane from the current native state plus an optional parallel-lists payload. */
  function lanes(input = {}) {
    const parallel = input.parallel && typeof input.parallel === "object" ? input.parallel : null;
    const built = [];
    built.push({
      id: "customers",
      label: "Customers",
      kind: "native",
      source: "NXT native ledger contacts",
      description: "People and businesses this book sells to. Balances come from this ledger's own documents.",
      route: "#customers",
      columns: columns.customer,
      rows: contactsOf(input.contacts, "customer")
    });
    built.push({
      id: "suppliers",
      label: "Suppliers",
      kind: "native",
      source: "NXT native ledger contacts",
      description: "Suppliers this book buys from. Balances come from this ledger's own documents.",
      route: "#vendors",
      columns: columns.customer,
      rows: contactsOf(input.contacts, "vendor")
    });
    built.push({
      id: "accounts",
      label: "Chart of accounts",
      kind: "native",
      source: "NXT native chart",
      description: "The chart this book posts to. Archived accounts stay listed and marked.",
      route: "#accounts",
      columns: columns.account,
      rows: list(input.accounts)
        .filter(row => row && row.code)
        .map(row => ({
          code: text(row.code),
          name: text(row.name),
          kind: KIND[row.kind] || text(row.kind),
          archived: row.archived === true
        }))
        .sort((a, b) => a.code.localeCompare(b.code, "en-CA", { numeric: true }))
    });

    if (parallel) {
      built.push({
        id: "employees",
        label: "Employees",
        kind: "parallel",
        source: "Company book parallel list",
        description: "Employee names recorded inside NXT for parallel testing. Payroll calculation, remittance and T4s are not enabled.",
        boundary: PARALLEL_BOUNDARY,
        columns: columns.employee,
        rows: list(parallel.employees).map(row => ({ name: text(row.name), active: row.active === true }))
      });
      built.push({
        id: "departments",
        label: "Departments",
        kind: "parallel",
        source: "Company book parallel list",
        description: "Departments recorded inside NXT for parallel work.",
        boundary: PARALLEL_BOUNDARY,
        columns: columns.department,
        rows: list(parallel.departments).map(row => ({ name: text(row.name) }))
      });
      built.push({
        id: "items",
        label: "Products and services",
        kind: "parallel",
        source: "Company book parallel list",
        description: "Items recorded inside NXT. Income accounts are plain labels; nothing is posted for you.",
        boundary: PARALLEL_BOUNDARY,
        columns: columns.item,
        rows: list(parallel.items).map(row => ({ name: text(row.name), income_account: text(row.income_account) }))
      });
      const names = new Map(list(parallel.employees).map(row => [row.id, text(row.name)]));
      built.push({
        id: "time",
        label: "Time entries",
        kind: "parallel",
        source: "Company book parallel list",
        description: "Time recorded inside NXT against a parallel employee. Nothing is billed, approved or posted to payroll.",
        boundary: PARALLEL_BOUNDARY,
        columns: columns.time,
        rows: list(parallel.time)
          .map(row => ({ employee: names.get(row.employee_id) || "", date: text(row.date), minutes: Number(row.minutes) || 0 }))
          .sort((a, b) => String(b.date).localeCompare(String(a.date)))
      });
      const people = new Map(list(input.contacts).map(row => [row.id, text(row.name)]));
      const docs = input.subledger && Array.isArray(input.subledger.documents) ? input.subledger.documents : [];
      const stubs = list(input.payroll && input.payroll.paystubs);
      if (stubs.length) {
        built.push({
          id: "paystubs",
          label: "Paystubs",
          kind: "parallel",
          source: "Company book payroll runs",
          description: "Pay runs recorded in this book with CPP, CPP2, EI and income tax withheld. Nothing is remitted, filed or posted to the ledger.",
          boundary: PARALLEL_BOUNDARY,
          columns: [{ key: "employee", label: "Employee" }, { key: "pay_date", label: "Pay date" }, { key: "gross", label: "Gross", num: 1 }, { key: "cpp", label: "CPP", num: 1 }, { key: "ei", label: "EI", num: 1 }, { key: "net", label: "Net", num: 1 }],
          rows: stubs.map(row => ({ employee: text(row.employee_name), pay_date: text(row.pay_date), gross: amount(row.gross_cents), cpp: amount(row.cpp_cents), ei: amount(row.ei_cents), net: amount(row.net_cents) }))
        });
      }
      const files = list(input.subledger && input.subledger.attachment_list);
      if (files.length) {
        built.push({
          id: "attachments",
          label: "Attachments",
          kind: "parallel",
          source: "Company book parallel list",
          description: "Files stored against parallel documents. NXT keeps the file and its size; it does not preview, download or write them back to QuickBooks.",
          boundary: PARALLEL_BOUNDARY,
          columns: [{ key: "filename", label: "File" }, { key: "owner", label: "Attached to" }, { key: "size", label: "Bytes", num: 1 }],
          rows: files.map(row => ({ filename: text(row.filename), owner: text(row.owner_id), size: text(row.byte_len) }))
        });
      }
      const recurring = list(input.recurring && input.recurring.recurring);
      if (recurring.length) {
        built.push({
          id: "recurring",
          label: "Recurring transactions",
          kind: "parallel",
          source: "Company book parallel list",
          description: "Recurring invoice and bill templates with a cadence and a next date. NXT stores the template and shows when it is next due; it never posts on its own.",
          boundary: PARALLEL_BOUNDARY,
          columns: [{ key: "kind", label: "Kind" }, { key: "contact", label: "Contact" }, { key: "amount", label: "Amount", num: 1 }, { key: "cadence", label: "Cadence" }, { key: "next_date", label: "Next date" }],
          rows: recurring.map(row => ({ kind: text(row.kind), contact: text(row.contact), amount: amount(row.amount_cents), cadence: text(row.cadence), next_date: text(row.next_date) }))
        });
      }
      const schedules = list(input.schedules && input.schedules.schedules);
      if (schedules.length) {
        built.push({
          id: "schedules",
          label: "Schedules",
          kind: "parallel",
          source: "Company book parallel list",
          description: "Straight-line schedules computed to the as-of date. NXT shows what has been amortised and what remains; nothing is posted to the ledger.",
          boundary: PARALLEL_BOUNDARY,
          columns: [{ key: "name", label: "Item" }, { key: "kind", label: "Kind" }, { key: "total", label: "Total", num: 1 }, { key: "monthly", label: "Monthly", num: 1 }, { key: "amortised", label: "Amortised", num: 1 }, { key: "remaining", label: "Remaining", num: 1 }],
          rows: schedules.map(row => ({
            name: text(row.name),
            kind: row.kind === "fixed_asset" ? "Fixed asset" : row.kind === "revenue_recognition" ? "Revenue recognition" : "Prepaid",
            total: amount(row.total_cents),
            monthly: amount(row.monthly_cents),
            amortised: amount(row.amortised_cents),
            remaining: amount(row.remaining_cents)
          }))
        });
      }
      const stock = list(input.inventory && input.inventory.items);
      if (stock.length) {
        built.push({
          id: "inventory",
          label: "Inventory",
          kind: "parallel",
          source: "Company book parallel list",
          description: "On-hand quantities from recorded stock movements, valued at the latest unit cost. Not weighted average or FIFO, and nothing posts to the ledger.",
          boundary: PARALLEL_BOUNDARY,
          columns: [{ key: "name", label: "Item" }, { key: "on_hand", label: "On hand", num: 1 }, { key: "cost", label: "Unit cost", num: 1 }, { key: "value", label: "Value", num: 1 }, { key: "moves", label: "Movements", num: 1 }],
          rows: stock.map(row => ({
            name: text(row.name),
            on_hand: (Number(row.on_hand_milli) / 1000).toFixed(3),
            cost: amount(row.unit_cost_cents),
            value: amount(row.value_cents),
            moves: text(row.movements)
          }))
        });
      }
      const trips = list(input.mileage && input.mileage.trips);
      if (trips.length) {
        built.push({
          id: "mileage",
          label: "Mileage",
          kind: "parallel",
          source: "Company book parallel list",
          description: "Trips recorded with a firm-supplied rate. NXT multiplies distance by the rate you enter; it does not claim a statutory rate.",
          boundary: PARALLEL_BOUNDARY,
          columns: [{ key: "date", label: "Date" }, { key: "purpose", label: "Purpose" }, { key: "km", label: "Km", num: 1 }, { key: "rate", label: "Rate/km", num: 1 }, { key: "amount", label: "Amount", num: 1 }],
          rows: trips.map(row => ({ date: text(row.date), purpose: text(row.purpose), km: (Number(row.kilometres_milli) / 1000).toFixed(3), rate: amount(row.rate_cents_per_km), amount: amount(row.amount_cents) }))
        });
      }
      const pipeline = list(input.crm && input.crm.records);
      if (pipeline.length) {
        built.push({
          id: "pipeline",
          label: "Sales pipeline",
          kind: "parallel",
          source: "Company book parallel list",
          description: "Opportunities, proposals and contracts recorded against contacts. NXT records the stage; it never sends, signs or books anything.",
          boundary: PARALLEL_BOUNDARY,
          columns: [{ key: "kind", label: "Kind" }, { key: "title", label: "Title" }, { key: "contact", label: "Contact" }, { key: "value", label: "Value", num: 1 }, { key: "status", label: "Status" }, { key: "date", label: "Date" }],
          rows: pipeline.map(row => ({ kind: text(row.kind), title: text(row.title), contact: text(row.contact), value: amount(row.value_cents), status: text(row.status), date: text(row.date) }))
        });
      }
      const salesDocs = list(input.salesDocs && input.salesDocs.documents);
      if (salesDocs.length) {
        built.push({
          id: "salesdocs",
          label: "Sales documents",
          kind: "parallel",
          source: "Company book parallel list",
          description: "Sales orders, sales receipts, refund receipts, delayed charges and delayed credits. Non-posting: nothing books, sends or posts.",
          boundary: PARALLEL_BOUNDARY,
          columns: [{ key: "kind", label: "Kind" }, { key: "contact", label: "Contact" }, { key: "number", label: "Number" }, { key: "total", label: "Total", num: 1 }],
          rows: salesDocs.map(row => ({ kind: text(row.kind).replace(/_/g, " "), contact: text(row.contact), number: text(row.document_number), total: amount(row.total_cents) }))
        });
      }
      const classes = list(input.subledger && input.subledger.class_list);
      if (classes.length) {
        built.push({
          id: "classes",
          label: "Classes",
          kind: "parallel",
          source: "Company book parallel list",
          description: "Class tracking recorded inside NXT. Classes can be attached to journal annotations; they never post on their own.",
          boundary: PARALLEL_BOUNDARY,
          columns: [{ key: "name", label: "Class" }],
          rows: classes.map(row => ({ name: text(row.name) })).sort((a, b) => a.name.localeCompare(b.name))
        });
      }
      if (docs.length) {
        built.push({
          id: "subledger",
          label: "Parallel documents",
          kind: "parallel",
          source: "Company book subledger",
          description: "Documents the parallel book holds, with what remains open after applied payments. Read-only list; select a row to post a payment.",
          boundary: PARALLEL_BOUNDARY,
          columns: columns.subledger,
          rows: docs.map(row => ({
            id: text(row.id),
            openCents: Number(row.open_cents) || 0,
      approval: text(row.approval) || "—",
            kind: text(row.kind),
            contact: people.get(row.contact) || "",
            total: amount(row.total_cents),
            open: amount(row.open_cents)
          }))
        });
      }
      const cash = new Map(list(input.accounts).map(row => [row.id, text(row.code) + (row.name ? " · " + text(row.name) : "")]));
      const nonPosting = (id, label, rows, description) => built.push({
        id, label, kind: "parallel", source: "Company book parallel list",
        description, boundary: PARALLEL_BOUNDARY, columns: columns.nonPosting,
        rows: list(rows).map(row => ({
          contact: people.get(row.contact) || "",
          document: text(row.document_number),
          lines: list(row.lines).length,
          total: amount(row.total_cents)
        }))
      });
      nonPosting("estimates", "Estimates", parallel.estimates, "Estimates recorded inside NXT. They are not invoices and nothing posts.");
      nonPosting("purchase-orders", "Purchase orders", parallel.purchase_orders, "Purchase orders recorded inside NXT. They are not bills and nothing posts.");
      built.push({
        id: "budgets",
        label: "Budgets",
        kind: "parallel",
        source: "Company book parallel list",
        description: "Monthly account budgets recorded inside NXT. They never change a posted figure or a report.",
        boundary: PARALLEL_BOUNDARY,
        columns: columns.budget,
        rows: list(parallel.budgets)
          .map(row => ({ account: cash.get(row.account_id) || "", period: text(row.period), amount: amount(row.amount_cents) }))
          .sort((a, b) => String(b.period).localeCompare(String(a.period)))
      });
    }
    return built;
  }

  const searchable = (row, column) => text(row[column.key]).toLowerCase();

  /** Case-insensitive search across the visible columns of one lane. */
  function filterLane(lane, query) {
    const q = text(query).toLowerCase().trim();
    if (!q) return list(lane.rows);
    return list(lane.rows).filter(row => list(lane.columns).some(column => searchable(row, column).includes(q)));
  }

  const NUMERIC_KEYS = new Set(["minutes", "total", "open", "amount", "lines", "terms_days"]);

  /** Sorts one lane by a known column key. Unknown keys and empty directions keep the incoming order. */
  function sortLane(rows, key, dir = "asc") {
    const rows_ = list(rows);
    if (!key) return rows_;
    const compare = NUMERIC_KEYS.has(key)
      ? (a, b) => (Number(a[key]) || 0) - (Number(b[key]) || 0)
      : (a, b) => text(a[key]).localeCompare(text(b[key]));
    const sorted = rows_.slice().sort(compare);
    return dir === "desc" ? sorted.reverse() : sorted;
  }

  function summary(lanesRows) {
    const rows = list(lanesRows);
    const laneRows = rows.reduce((total, lane) => total + list(lane.rows).length, 0);
    const parallel = rows.filter(lane => lane.kind === "parallel").reduce((total, lane) => total + list(lane.rows).length, 0);
    return { lanes: rows.length, rows: laneRows, native: laneRows - parallel, parallel };
  }

  /** Cell text as exported: booleans become Yes/No, empty stays empty rather than zero. */
  function cell(column, row) {
    const value = row[column.key];
    if (column.yesno) return value === true ? "Yes" : "No";
    if (value === null || value === undefined) return "";
    return text(value);
  }

  /** CSV rows for one lane, header first. Values keep their exact typed form. */
  function laneCsv(lane) {
    return [list(lane.columns).map(column => column.label), ...list(lane.rows).map(row => list(lane.columns).map(column => cell(column, row)))];
  }

  /** One combined export with section headers, so an operator can hand over a single file. */
  function combinedCsv(lanesRows) {
    const out = [];
    for (const lane of list(lanesRows)) {
      out.push([lane.label]);
      out.push(...laneCsv(lane));
      out.push([]);
    }
    return out;
  }

  function fileName(scope, lane) {
    const base = text(scope).trim() ? text(scope).trim().replace(/[^A-Za-z0-9._-]+/g, "-") : "firm-workspace";
    return (lane ? base + "-" + lane + ".csv" : base + "-lists.csv");
  }

  /** Renders the lane cards. `table` and `esc` are injected so this module owns no page shell. */
  function renderLanes(lanesRows, helpers = {}) {
    const table = helpers.table, esc = helpers.esc || (value => text(value));
    if (typeof table !== "function") throw Error("renderLanes needs a table renderer");
    const query = text(helpers.query);
    return list(lanesRows).map(lane => {
      const filtered = filterLane(lane, query);
      const sort = helpers.sort && list(lane.columns).some(column => column.key === helpers.sort.key) ? helpers.sort : null;
      const rows = sort ? sortLane(filtered, sort.key, sort.dir) : filtered;
      const cols = list(lane.columns).map(column => ({
        h: column.label,
        num: !!column.num,
        sort: column.key,
        sortDir: sort && sort.key === column.key ? sort.dir : null,
        f: row => column.yesno ? (row[column.key] === true ? "Yes" : "No") : esc(cell(column, row))
      }));
      const empty = query ? "No row matches this search." : "This list is empty.";
      const onClick = helpers.onRow && lane.id === "subledger" ? (row) => helpers.onRow(lane, row) : undefined;
      return `<section class="card" data-lane="${esc(lane.id)}">
        <div class="card-head"><div><h2>${esc(lane.label)} <span class="pill neutral">${rows.length}</span></h2>
        <p class="form-note" style="margin:6px 0 0">${esc(lane.description)}</p></div>
        <div class="row"><span class="pill ${lane.kind === "parallel" ? "archived" : "approved"}">${esc(lane.source)}</span>
        ${lane.route ? `<a class="btn sm" href="${esc(lane.route)}">Open</a>` : ""}
        ${canAdd(lane) ? `<button class="btn sm" type="button" data-add-lane="${esc(lane.id)}">Add</button>` : ""}
        <button class="btn sm" type="button" data-export-lane="${esc(lane.id)}">Export</button></div></div>
        ${table(cols, rows, { empty, onClick })}
        ${lane.boundary ? `<p class="form-note" style="margin:10px 0 0">${esc(lane.boundary)}</p>` : ""}</section>`;
    }).join("");
  }

  window.NXTLedgerLists = { lanes, filterLane, summary, laneCsv, combinedCsv, cell, fileName, renderLanes, sortLane, createRequest, createTimeRequest, createBudgetRequest, createDocumentRequest, createPaymentRequest, nameError, canAdd, requestId, amount, PARALLEL_BOUNDARY };
})();
