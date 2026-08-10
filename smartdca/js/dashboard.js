let lastPrice = 0;
let botStartTime = null;
let isBalanceHistoryInitialized = false;
let showAllBalanceHistory = false;
let selectedSymbol = 'BTCUSDT';
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
    const sections = ['overview', 'cycles', 'history', 'scanner'];
    sections.forEach(s => {
        const el = document.getElementById(`section-${s}`);
        if (el) el.style.display = (s === tabName) ? '' : 'none';
        const btn = document.getElementById(`btn-page-${s}`);
        if (btn) btn.classList.toggle('active', s === tabName);
    });
}
window.switchPageTab = switchPageTab;

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
window.toggleBalanceHistoryRange = toggleBalanceHistoryRange;

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

// ─── Symbol Selection ────────────────────────────────────────────────────────
async function changeSelectedSymbol(val) {
    selectedSymbol = val;
    lastPrice = 0; // reset price flashing
    await Promise.all([
        fetchStatus(),
        fetchHistory(),
        fetchCycles()
    ]);
}
window.changeSelectedSymbol = changeSelectedSymbol;

// ─── Status Fetch ─────────────────────────────────────────────────────────────
async function fetchStatus() {
    try {
        const res = await fetch(`${apiBase}api/status?symbol=${selectedSymbol}`);
        const data = await res.json();

        if (!botStartTime) {
            botStartTime = new Date(data.start_time);
        }

        // Update static coin label
        const coinLabel = document.getElementById('current-selected-coin');
        if (coinLabel) {
            coinLabel.innerText = selectedSymbol;
        }

        // Update token name text tags
        const baseAsset = selectedSymbol.replace('USDT', '');
        const cleanTags = ['token-name-holdings', 'token-name-price', 'token-name-hwm'];
        cleanTags.forEach(id => {
            const el = document.getElementById(id);
            if (el) el.innerText = baseAsset;
        });

        // Update active positions table
        const activeTbody = document.getElementById('active-positions-table-body');
        if (activeTbody && data.active_positions) {
            if (data.active_positions.length === 0) {
                activeTbody.innerHTML = `<tr><td colspan="7" style="text-align: center; color: var(--text-secondary); padding: 15px;">Tidak ada koin aktif — No active positions</td></tr>`;
            } else {
                activeTbody.innerHTML = data.active_positions.map(pos => {
                    const pnlVal = pos.current_pnl_pct;
                    const pnlColor = pnlVal >= 0 ? 'var(--accent-green)' : 'var(--accent-red)';
                    const pnlSign = pnlVal >= 0 ? '+' : '';
                    const isSelected = pos.symbol === selectedSymbol;
                    const rowBg = isSelected ? 'background: rgba(15, 76, 129, 0.05);' : '';
                    return `
                        <tr style="${rowBg}">
                            <td style="font-weight: bold; color: var(--accent-blue);">${pos.symbol}</td>
                            <td>#${pos.cycle_id}</td>
                            <td style="font-weight: bold;">${pos.layers_filled}/3</td>
                            <td>${fmtUSD(pos.avg_entry_price)}</td>
                            <td>${fmtUSD(pos.current_price)}</td>
                            <td>
                                <span style="color: ${pnlColor}; font-weight: bold;">${pnlSign}${pnlVal.toFixed(2)}%</span>
                            </td>
                            <td>
                                <button class="btn" style="padding: 4px 8px; font-size: 0.8em; font-weight: bold;" onclick="changeSelectedSymbol('${pos.symbol}');">Chart</button>
                            </td>
                        </tr>
                    `;
                }).join('');
            }
        }

        // Update general top metrics cards
        const equityVal = document.getElementById('equity-val');
        if (equityVal) equityVal.innerText = fmtUSD(data.total_equity);
        const leveragedEquityVal = document.getElementById('leveraged-equity-val');
        if (leveragedEquityVal) leveragedEquityVal.innerText = fmtUSD(data.total_equity * 3.0);
        const marginSpentVal = document.getElementById('margin-spent-val');
        if (marginSpentVal) marginSpentVal.innerText = fmtUSD(data.total_margin_spent || 0.0);
        const balanceVal = document.getElementById('balance-val');
        if (balanceVal) balanceVal.innerText = fmtUSD(data.simulated_balance);

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
            tbody.innerHTML = `<tr><td colspan="7" style="text-align: center; color: var(--text-secondary);">Belum ada riwayat siklus selesai — No completed cycle history</td></tr>`;
            return;
        }

        tbody.innerHTML = data.map(cycle => {
            const pnlClass = cycle.status === 'WIN' ? 'badge-win' : 'badge-loss';
            const prefix = (cycle.net_pnl || 0) >= 0 ? '+' : '';
            const pnlPctPrefix = (cycle.pnl_pct || 0) >= 0 ? '+' : '';
            return `
                <tr>
                    <td style="font-weight: bold; color: var(--accent-blue);">${cycle.symbol}</td>
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
            tbody.innerHTML = `<tr><td colspan="9" style="text-align: center; color: var(--text-secondary);">Belum ada transaksi — No trade history</td></tr>`;
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

            // Modal/Capital Cell (COALESCE handles c.total_spent for SELL trades)
            const modalCell = trade.usdt_spent != null ? fmtUSD(trade.usdt_spent) : '-';

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
            }

            return `
                <tr style="${rowStyle}">
                    <td>${date}</td>
                    <td style="font-weight: bold; color: var(--accent-blue);">${trade.symbol}</td>
                    <td>#${trade.cycle_id}</td>
                    <td class="${actionClass}" style="font-weight:bold;">${trade.action}</td>
                    <td>${layerStr}</td>
                    <td>${fmtUSD(trade.price)}</td>
                    <td>${trade.amount.toFixed(6)}</td>
                    <td style="font-weight: bold;">${modalCell}</td>
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
        const res = await fetch('api/manual_sell', { 
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ symbol: selectedSymbol })
        });
        if (res.ok) {
            alert("Emergency Manual Sell berhasil dieksekusi!");
            fetchStatus();
            fetchHistory();
            fetchCycles();
        } else {
            const text = await res.text();
            alert("Gagal melakukan manual sell: " + text);
        }
    } catch (e) {
        alert("Error menghubungi server: " + e);
    }
}
window.triggerManualSell = triggerManualSell;

// ─── Volatility Scanner ────────────────────────────────────────────────────────
async function fetchScanner() {
    try {
        const res = await fetch(`${apiBase}api/scanner`);
        const data = await res.json();
        const tbody = document.getElementById('scanner-table-body');
        if (!tbody) return;

        if (!data || data.length === 0) {
            tbody.innerHTML = `<tr><td colspan="7" style="text-align: center; color: var(--text-secondary);">Tidak ada data pemindai</td></tr>`;
            return;
        }

        tbody.innerHTML = data.map((candidate, idx) => {
            const statusBadge = candidate.is_active
                ? `<span style="border: 1px solid var(--accent-green); color: var(--accent-green); padding: 2px 6px; border-radius: 4px; font-size: 0.85em; font-weight: bold; background: rgba(46, 204, 113, 0.05);">MONITORED</span>`
                : `<span style="color: var(--text-secondary); border: 1px solid var(--border-color); padding: 2px 6px; border-radius: 4px; font-size: 0.85em; background: rgba(0, 0, 0, 0.02);">IDLE</span>`;

            const layersStr = candidate.layers_filled > 0
                ? `<span style="color: var(--accent-red); font-weight: bold;">POSITION (${candidate.layers_filled}/3)</span>`
                : `<span style="color: var(--text-secondary);">None (0/3)</span>`;

            const rank = idx + 1;
            const rankStyle = rank <= 5 ? 'font-weight: bold; color: var(--accent-blue);' : '';

            return `
                <tr>
                    <td style="${rankStyle}">#${rank}</td>
                    <td style="font-weight: bold; color: var(--accent-blue); font-family: monospace;">${candidate.symbol}</td>
                    <td style="font-family: monospace; font-weight: bold;">${candidate.volatility.toFixed(2)}%</td>
                    <td style="font-family: monospace;">${fmtUSD(candidate.volume)}</td>
                    <td>${statusBadge}</td>
                    <td>${layersStr}</td>
                    <td>
                        <button class="btn" style="padding: 4px 8px; font-size: 0.85em; font-weight: bold;" onclick="changeSelectedSymbol('${candidate.symbol}'); switchPageTab('overview');">View</button>
                    </td>
                </tr>
            `;
        }).join('');
    } catch (e) {
        console.error("Gagal fetch scanner:", e);
    }
}

// ─── Refresh Loops ────────────────────────────────────────────────────────────
setInterval(fetchStatus, 1000);
setInterval(fetchLogs, 2000);
setInterval(fetchScanner, 5000); // Poll scanner every 5 seconds for instant feedback
setInterval(fetchHistory, 10000);
setInterval(fetchCycles, 10000);
setInterval(fetchBalanceHistory, 15000);

// ─── Initial Loads ────────────────────────────────────────────────────────────
fetchStatus();
fetchLogs();
fetchScanner();
fetchHistory();
fetchCycles();
fetchBalanceHistory();
