let lastPrice = 0;
let botStartTime = null;
let isBalanceHistoryInitialized = false;
let showAllBalanceHistory = false;
const apiBase = window.location.protocol === 'file:' ? 'https://tradingsafe.mijdigital.my/dca/' : '';

// Initialize Equity Chart
const ctxEquity = document.getElementById('equityChart').getContext('2d');
const equityChart = new Chart(ctxEquity, {
    type: 'line',
    data: {
        labels: [],
        datasets: [{
            borderColor: '#0f4c81',
            borderWidth: 1.8,
            data: [],
            fill: false,
            tension: 0.3,
            pointRadius: 0
        }]
    },
    options: {
        responsive: true,
        maintainAspectRatio: false,
        plugins: { legend: { display: false } },
        scales: {
            x: { ticks: { color: 'var(--text-secondary, #666)' }, grid: { color: 'rgba(0,0,0,0.05)' } },
            y: { ticks: { color: 'var(--text-secondary, #666)' }, grid: { color: 'rgba(0,0,0,0.05)' } }
        }
    }
});

const fmtUSD = (val) => new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(val);

// ─── Page Tab Switching ───────────────────────────────────────────────────────
function switchPageTab(tabName) {
    const sections = ['overview', 'cycles', 'history'];
    sections.forEach(s => {
        const el = document.getElementById(`section-${s}`);
        if (el) el.style.display = (s === tabName) ? '' : 'none';
        const btn = document.getElementById(`btn-page-${s}`);
        if (btn) btn.classList.toggle('active', s === tabName);
    });
}

// ─── Chart All-Data Toggle ────────────────────────────────────────────────────
async function toggleBalanceHistoryRange() {
    showAllBalanceHistory = !showAllBalanceHistory;
    const btn = document.getElementById('btn-chart-all');
    if (btn) {
        btn.classList.toggle('active', showAllBalanceHistory);
        btn.innerHTML = showAllBalanceHistory
            ? 'Default (1 Minggu) <span style="font-size:0.85em;font-weight:normal;opacity:0.8;margin-left:3px;">(Show 1 Week)</span>'
            : 'Tampilkan Semua Data <span style="font-size:0.85em;font-weight:normal;opacity:0.8;margin-left:3px;">(Show All Data)</span>';
    }
    isBalanceHistoryInitialized = false;
    await fetchBalanceHistory();
}

// ─── Uptime Counter ───────────────────────────────────────────────────────────
setInterval(() => {
    if (!botStartTime) return;
    const now = new Date();
    const diffMs = now - botStartTime;
    const diffSec = Math.floor(diffMs / 1000);
    const hours = String(Math.floor(diffSec / 3600)).padStart(2, '0');
    const minutes = String(Math.floor((diffSec % 3600) / 60)).padStart(2, '0');
    const seconds = String(diffSec % 60).padStart(2, '0');
    const uptimeEl = document.getElementById('uptime-counter');
    if (uptimeEl) uptimeEl.innerText = `${hours}:${minutes}:${seconds}`;
}, 1000);

// ─── Status Fetch ─────────────────────────────────────────────────────────────
async function fetchStatus() {
    try {
        const res = await fetch(`${apiBase}api/status`);
        const data = await res.json();

        if (!botStartTime) {
            botStartTime = new Date(data.start_time);
        }

        // Price flashing effect
        const priceEl = document.getElementById('btc-price-val');
        if (priceEl) {
            priceEl.innerText = fmtUSD(data.current_price);
            if (lastPrice > 0 && data.current_price !== lastPrice) {
                priceEl.classList.add('flashing');
                setTimeout(() => priceEl.classList.remove('flashing'), 500);
            }
            lastPrice = data.current_price;
        }

        // Primary Metrics
        document.getElementById('equity-val').innerText = fmtUSD(data.total_equity);
        document.getElementById('balance-val').innerText = fmtUSD(data.simulated_balance);
        document.getElementById('btc-val').innerText = data.btc_balance.toFixed(6);
        document.getElementById('btc-value-usd').innerText = fmtUSD(data.btc_balance * data.current_price) + " USD";

        // Layers Metric
        document.getElementById('layers-val').innerText = `${data.layers_filled} / 3`;
        let dots = "○○○";
        if (data.layers_filled === 1) dots = "●○○";
        else if (data.layers_filled === 2) dots = "●●○";
        else if (data.layers_filled === 3) dots = "●●●";
        document.getElementById('layer-dots').innerText = dots;

        // Active Position Details
        document.getElementById('cycle-id').innerText = `#${data.current_cycle_id}`;
        document.getElementById('avg-entry-val').innerText = data.avg_entry_price > 0 ? fmtUSD(data.avg_entry_price) : "-";
        document.getElementById('hwm-val').innerText = data.cycle_high_water_mark > 0 ? fmtUSD(data.cycle_high_water_mark) : "-";

        // PNL Display
        const pnlValEl = document.getElementById('pnl-pct-val');
        if (data.avg_entry_price > 0 && data.btc_balance > 0) {
            const prefix = data.current_pnl_pct >= 0 ? '+' : '';
            pnlValEl.innerText = `${prefix}${data.current_pnl_pct.toFixed(2)}%`;
            if (data.current_pnl_pct > 0) {
                pnlValEl.style.color = 'var(--accent-green)';
            } else if (data.current_pnl_pct < 0) {
                pnlValEl.style.color = 'var(--accent-red)';
            } else {
                pnlValEl.style.color = 'var(--text-primary)';
            }
        } else {
            pnlValEl.innerText = "0.00%";
            pnlValEl.style.color = 'var(--text-secondary)';
        }

        // Header Metrics
        const winrateEl = document.getElementById('winrate-counter');
        if (winrateEl) winrateEl.innerText = `${data.winrate.toFixed(1)}%`;
        const cpuEl = document.getElementById('cpu-counter');
        if (cpuEl) cpuEl.innerText = `${data.sys_cpu_pct.toFixed(1)}%`;
        const ramEl = document.getElementById('ram-counter');
        if (ramEl) ramEl.innerText = `${data.sys_mem_mb.toFixed(0)} MB`;

        // LED Indicators
        setLED('led-ws', data.ws_active);
        setLED('led-conclude', data.conclude_active);
        setLED('led-validate', data.validate_active);
        setLED('led-executor', data.executor_active);

        // Real-time chart update — only if changed
        if (!showAllBalanceHistory) {
            const currentBalance = data.total_equity;
            const balanceData = equityChart.data.datasets[0].data;
            const lastPlottedBalance = balanceData[balanceData.length - 1];
            if (lastPlottedBalance === undefined || Math.abs(lastPlottedBalance - currentBalance) > 0.01) {
                const timeStr = new Date().toLocaleTimeString('id-ID', { hour: '2-digit', minute: '2-digit' });
                equityChart.data.labels.push(timeStr);
                equityChart.data.datasets[0].data.push(currentBalance);
                if (equityChart.data.labels.length > 100) {
                    equityChart.data.labels.shift();
                    equityChart.data.datasets[0].data.shift();
                }
                equityChart.update('none');
            }
        }

    } catch (e) {
        console.error("Gagal fetch status API:", e);
    }
}

function setLED(id, active) {
    const el = document.getElementById(id);
    if (!el) return;
    el.className = active ? "led-light active-green" : "led-light active-red";
}

// ─── Balance History ──────────────────────────────────────────────────────────
async function fetchBalanceHistory() {
    if (isBalanceHistoryInitialized) return;
    try {
        const url = showAllBalanceHistory
            ? `${apiBase}api/balance?all=true`
            : `${apiBase}api/balance`;
        const res = await fetch(url);
        const data = await res.json();

        if (!data || data.length === 0) return;

        const labels = data.map(item => {
            const d = new Date(item.timestamp);
            return d.toLocaleDateString('id-ID', { month: 'short', day: 'numeric' })
                 + ' ' + d.toLocaleTimeString('id-ID', { hour: '2-digit', minute: '2-digit' });
        });
        const values = data.map(item => item.total_value);

        // Safeguard against flat-line NaN issue on mobile
        const minVal = Math.min(...values);
        const maxVal = Math.max(...values);
        const yOptions = {};
        if (maxVal - minVal < 0.01) {
            yOptions.min = minVal - 10;
            yOptions.max = maxVal + 10;
        }

        equityChart.options.scales.y = {
            ...equityChart.options.scales.y,
            ...yOptions
        };
        equityChart.data.labels = labels;
        equityChart.data.datasets[0].data = values;
        equityChart.update();
        isBalanceHistoryInitialized = true;
    } catch (e) {
        console.error("Gagal fetch balance history:", e);
    }
}

// ─── DCA Cycles ───────────────────────────────────────────────────────────────
async function fetchCycles() {
    try {
        const res = await fetch(`${apiBase}api/cycles`);
        const data = await res.json();
        const tbody = document.getElementById('cycles-table-body');
        if (!tbody) return;

        if (data.length === 0) {
            tbody.innerHTML = `<tr><td colspan="6" style="text-align: center; color: var(--text-secondary);">Belum ada riwayat siklus selesai — No completed cycle history</td></tr>`;
            return;
        }

        tbody.innerHTML = data.map(cycle => {
            const pnlClass = cycle.status === 'WIN' ? 'badge-win' : 'badge-loss';
            const prefix = (cycle.net_pnl || 0) >= 0 ? '+' : '';
            const pnlPctPrefix = (cycle.pnl_pct || 0) >= 0 ? '+' : '';
            return `
                <tr>
                    <td>#${cycle.cycle_id}</td>
                    <td>${fmtUSD(cycle.avg_entry_price || 0)}</td>
                    <td>${fmtUSD(cycle.exit_price || 0)}</td>
                    <td class="${pnlClass}">${prefix}${fmtUSD(cycle.net_pnl || 0)}</td>
                    <td class="${pnlClass}">${pnlPctPrefix}${(cycle.pnl_pct || 0).toFixed(2)}%</td>
                    <td class="${pnlClass}">${cycle.status || '-'}</td>
                </tr>
            `;
        }).join('');
    } catch (e) {
        console.error("Gagal fetch cycles:", e);
    }
}

// ─── Trade History ────────────────────────────────────────────────────────────
async function fetchHistory() {
    try {
        const res = await fetch(`${apiBase}api/history`);
        const data = await res.json();
        const tbody = document.getElementById('history-table-body');
        if (!tbody) return;

        if (data.length === 0) {
            tbody.innerHTML = `<tr><td colspan="7" style="text-align: center; color: var(--text-secondary);">Belum ada transaksi — No trade history</td></tr>`;
            return;
        }

        tbody.innerHTML = data.map(trade => {
            const date = new Date(trade.timestamp).toLocaleString('id-ID', {
                month: 'short', day: 'numeric',
                hour: '2-digit', minute: '2-digit', second: '2-digit'
            });
            const isBuy = trade.action === 'BUY';
            const isSell = trade.action === 'SELL';

            // Row highlight color
            const rowStyle = isSell
                ? `background: rgba(46, 204, 113, 0.06); border-left: 3px solid var(--accent-green);`
                : `border-left: 3px solid var(--accent-blue, #0f4c81);`;
            const actionClass = isBuy ? 'badge-win' : 'badge-loss';
            const layerStr = trade.layer ? `L${trade.layer}` : '-';

            // PNL cell for SELL rows
            let pnlCell = '-';
            if (isSell && trade.net_pnl != null) {
                const pnl = trade.net_pnl;
                const pct = trade.pnl_pct;
                const pnlColor = pnl >= 0 ? 'var(--accent-green)' : 'var(--accent-red)';
                const pnlSign = pnl >= 0 ? '+' : '';
                const pctSign = pct >= 0 ? '+' : '';
                pnlCell = `<span style="color:${pnlColor}; font-weight: bold;">
                    ${pnlSign}${fmtUSD(pnl)}<br>
                    <span style="font-size:0.85em;">${pctSign}${pct.toFixed(2)}%</span>
                </span>`;
            } else if (isBuy) {
                const spent = trade.usdt_spent != null ? fmtUSD(trade.usdt_spent) : '-';
                pnlCell = `<span style="color: var(--text-secondary); font-size:0.9em;">${spent}</span>`;
            }

            return `
                <tr style="${rowStyle}">
                    <td>${date}</td>
                    <td>#${trade.cycle_id}</td>
                    <td class="${actionClass}" style="font-weight:bold;">${trade.action}</td>
                    <td>${layerStr}</td>
                    <td>${fmtUSD(trade.price)}</td>
                    <td>${trade.amount.toFixed(6)}</td>
                    <td>${pnlCell}</td>
                </tr>
            `;
        }).join('');
    } catch (e) {
        console.error("Gagal fetch history:", e);
    }
}

// ─── Logs ─────────────────────────────────────────────────────────────────────
async function fetchLogs() {
    try {
        const res = await fetch(`${apiBase}api/logs`);
        const logs = await res.json();
        const logBox = document.getElementById('log-box');
        if (!logBox) return;
        logBox.innerText = logs.join('\n');
        logBox.scrollTop = logBox.scrollHeight;
    } catch (e) {
        console.error("Gagal fetch logs:", e);
    }
}

// ─── Manual Sell ──────────────────────────────────────────────────────────────
async function triggerManualSell() {
    if (!confirm("Anda yakin ingin melakukan Emergency Manual Sell untuk menutup siklus saat ini?\n\nAre you sure you want to trigger an Emergency Manual Sell?")) {
        return;
    }
    try {
        const res = await fetch('api/manual_sell', { method: 'POST' });
        if (res.ok) {
            alert("Emergency Manual Sell berhasil dieksekusi!");
            fetchStatus();
            fetchHistory();
        } else {
            const text = await res.text();
            alert("Gagal melakukan manual sell: " + text);
        }
    } catch (e) {
        alert("Error menghubungi server: " + e);
    }
}

// ─── Refresh Loops ────────────────────────────────────────────────────────────
setInterval(fetchStatus, 1000);
setInterval(fetchLogs, 2000);
setInterval(fetchHistory, 10000);
setInterval(fetchCycles, 10000);
setInterval(fetchBalanceHistory, 15000);

// ─── Initial Loads ────────────────────────────────────────────────────────────
fetchStatus();
fetchLogs();
fetchHistory();
fetchCycles();
fetchBalanceHistory();
