function getApiPrefix() {
    const path = window.location.pathname;
    if (path.startsWith('/statarb') || path.indexOf('/statarb/') !== -1) {
        return '/statarb/';
    }
    return '/';
}

// Uptime & System Ticks
let startTime = null;

// Chart objects
let zscoreChartObj = null;
let equityChartObj = null;

// Initialize charts
function initCharts() {
    const ctxZ = document.getElementById('zscoreChart').getContext('2d');
    zscoreChartObj = new Chart(ctxZ, {
        type: 'line',
        data: {
            labels: [],
            datasets: [
                {
                    label: 'Z-Score',
                    data: [],
                    borderColor: '#0f4c81',
                    borderWidth: 2,
                    fill: false,
                    tension: 0.1,
                    pointRadius: 0
                },
                {
                    label: 'Upper Threshold (+2.0)',
                    data: [],
                    borderColor: '#b91c1c',
                    borderWidth: 1,
                    borderDash: [5, 5],
                    fill: false,
                    pointRadius: 0
                },
                {
                    label: 'Lower Threshold (-2.0)',
                    data: [],
                    borderColor: '#2e7d32',
                    borderWidth: 1,
                    borderDash: [5, 5],
                    fill: false,
                    pointRadius: 0
                }
            ]
        },
        options: {
            responsive: true,
            maintainAspectRatio: false,
            scales: {
                y: {
                    min: -4,
                    max: 4,
                    grid: {
                        color: '#e5dfd5'
                    }
                },
                x: {
                    grid: {
                        display: false
                    },
                    ticks: {
                        maxRotation: 0,
                        autoSkip: true,
                        maxTicksLimit: 10
                    }
                }
            },
            plugins: {
                legend: {
                    display: true,
                    labels: {
                        font: {
                            family: 'Georgia'
                        }
                    }
                }
            }
        }
    });

    const ctxE = document.getElementById('equityChart').getContext('2d');
    equityChartObj = new Chart(ctxE, {
        type: 'line',
        data: {
            labels: [],
            datasets: [{
                label: 'Total Equity (USDT)',
                data: [],
                borderColor: '#0f4c81',
                backgroundColor: 'rgba(15, 76, 129, 0.05)',
                borderWidth: 2,
                fill: true,
                tension: 0.1,
                pointRadius: 1
            }]
        },
        options: {
            responsive: true,
            maintainAspectRatio: false,
            scales: {
                y: {
                    grid: {
                        color: '#e5dfd5'
                    }
                },
                x: {
                    grid: {
                        display: false
                    },
                    ticks: {
                        maxRotation: 0,
                        autoSkip: true,
                        maxTicksLimit: 10
                    }
                }
            },
            plugins: {
                legend: {
                    display: false
                }
            }
        }
    });
}

// Format numbers
function formatUSDT(value) {
    return new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(value);
}

// Fetch live metrics every 1 second
async function fetchStatus() {
    try {
        const res = await fetch(getApiPrefix() + 'api/status');
        if (!res.ok) throw new Error('API server down');
        
        const data = await res.json();
        
        // Update stats
        document.getElementById('simulated-balance').innerText = formatUSDT(data.simulated_balance);
        document.getElementById('deployed-balance').innerText = formatUSDT(data.total_deployed_usdt);
        document.getElementById('total-equity').innerText = formatUSDT(data.total_equity);
        document.getElementById('realized-pnl').innerText = formatUSDT(data.total_pnl);
        document.getElementById('total-trades').innerText = data.total_trades;

        // Color realized P&L
        const pnlEl = document.getElementById('realized-pnl');
        if (data.total_pnl > 0) {
            pnlEl.className = 'value text-pnl-up';
        } else if (data.total_pnl < 0) {
            pnlEl.className = 'value text-pnl-down';
        } else {
            pnlEl.className = 'value';
        }

        // Set start time
        if (!startTime) {
            startTime = new Date(data.start_time);
        }

        // Update process LEDs (with data feed health check)
        toggleLED('led-ws', data.ws_active && data.data_feed_healthy);
        toggleLED('led-anls', data.engine_active);
        toggleLED('led-val', data.engine_active);
        toggleLED('led-exec', data.engine_active);
        toggleLED('led-corr', data.corrector_active);

        // Warmup status styling on main status LED
        const isWarmingUp = data.warmup_progress !== "READY";
        const mainLed = document.getElementById('main-status-led');
        const sigRec = document.getElementById('signal-recommendation');

        if (isWarmingUp) {
            if (mainLed) {
                mainLed.className = 'led-light';
                mainLed.style.backgroundColor = '#f59e0b'; // Orange
                mainLed.style.boxShadow = '0 0 8px #f59e0b';
            }
            if (sigRec) {
                sigRec.innerText = 'WARMING UP (' + data.warmup_progress + ')';
                sigRec.style.color = '#f59e0b';
                sigRec.style.fontWeight = 'bold';
            }
        } else {
            if (mainLed) {
                mainLed.style.backgroundColor = '';
                mainLed.style.boxShadow = '';
                toggleLED('main-status-led', data.engine_active && data.ws_active && data.data_feed_healthy);
            }
            if (sigRec) {
                sigRec.style.color = '';
                sigRec.style.fontWeight = '';
            }
        }

        // Fetch enterprise metrics
        const metricsRes = await fetch(getApiPrefix() + 'api/metrics');
        if (metricsRes.ok) {
            const metrics = await metricsRes.json();
            const elWinRate = document.getElementById('metrics-win-rate');
            if (elWinRate) elWinRate.innerText = metrics.win_rate.toFixed(1) + '%';
            
            const elFeeDrag = document.getElementById('metrics-fee-drag');
            if (elFeeDrag) elFeeDrag.innerText = metrics.avg_fee_drag_pct.toFixed(2) + '%';
            
            const elAvgCapture = document.getElementById('metrics-avg-capture');
            if (elAvgCapture) elAvgCapture.innerText = formatUSDT(metrics.avg_gross_capture_usd) + ' / ' + formatUSDT(metrics.avg_fee_usd);
            
            const elProfitFactor = document.getElementById('metrics-profit-factor');
            if (elProfitFactor) elProfitFactor.innerText = metrics.profit_factor.toFixed(2);
        }

    } catch (err) {
        console.error('Error fetching status:', err);
        toggleLED('main-status-led', false);
    }
}

function toggleLED(id, active) {
    const el = document.getElementById(id);
    if (!el) return;
    if (active) {
        el.className = 'led-light active-green';
    } else {
        el.className = 'led-light active-red';
    }
}

// Fetch active positions and pair stats
async function fetchTickData() {
    try {
        // Fetch pair stats
        const pairRes = await fetch(getApiPrefix() + 'api/pair_stats');
        if (pairRes.ok) {
            const list = await pairRes.json();
            if (list.length > 0) {
                const latest = list[list.length - 1];
                
                // Update text elements
                document.getElementById('price-a').innerText = formatUSDT(latest.price_a);
                document.getElementById('price-b').innerText = formatUSDT(latest.price_b);
                document.getElementById('current-ratio').innerText = latest.current_ratio.toFixed(6);
                document.getElementById('rolling-mean').innerText = latest.rolling_mean.toFixed(6);
                document.getElementById('rolling-std').innerText = latest.rolling_std.toFixed(6);

                // Update beta & r2
                const elBeta = document.getElementById('ols-beta');
                if (elBeta) elBeta.innerText = (latest.beta || 0).toFixed(6);
                const elR2 = document.getElementById('ols-r2');
                if (elR2) elR2.innerText = ((latest.r2 || 0) * 100).toFixed(2) + '%';

                const zVal = latest.z_score;
                const zEl = document.getElementById('z-score-val');
                zEl.innerText = zVal.toFixed(2);

                const statusRes = await fetch(getApiPrefix() + 'api/status');
                let isWarmingUp = false;
                if (statusRes.ok) {
                    const statusData = await statusRes.json();
                    isWarmingUp = statusData.warmup_progress !== "READY";
                }

                if (!isWarmingUp) {
                    const signalEl = document.getElementById('signal-recommendation');
                    if (zVal > 2.0) {
                        zEl.style.color = '#b91c1c';
                        signalEl.innerText = 'SELL SPREAD (SHORT ETH / LONG BTC)';
                        signalEl.className = 'label text-sell';
                        toggleLED('pair-status-led', true);
                        document.getElementById('pair-status-led').className = 'led-light active-red';
                    } else if (zVal < -2.0) {
                        zEl.style.color = '#2e7d32';
                        signalEl.innerText = 'BUY SPREAD (LONG ETH / SHORT BTC)';
                        signalEl.className = 'label text-buy';
                        toggleLED('pair-status-led', true);
                    } else {
                        zEl.style.color = '#1a1a1a';
                        signalEl.innerText = 'NEUTRAL (WAIT)';
                        signalEl.className = 'label';
                        document.getElementById('pair-status-led').className = 'led-light';
                    }
                }

                // Update Z-Score Chart
                const labels = list.map(item => new Date(item.timestamp).toLocaleTimeString());
                const zScores = list.map(item => item.z_score);
                const upperLimit = Array(list.length).fill(2.0);
                const lowerLimit = Array(list.length).fill(-2.0);

                zscoreChartObj.data.labels = labels;
                zscoreChartObj.data.datasets[0].data = zScores;
                zscoreChartObj.data.datasets[1].data = upperLimit;
                zscoreChartObj.data.datasets[2].data = lowerLimit;
                zscoreChartObj.update('none');
            }
        }

        // Fetch active positions
        const posRes = await fetch(getApiPrefix() + 'api/positions');
        if (posRes.ok) {
            const list = await posRes.json();
            const tbody = document.getElementById('active-positions-body');
            tbody.innerHTML = '';

            if (list.length === 0) {
                tbody.innerHTML = `<tr><td colspan="9" style="text-align: center; color: var(--text-secondary);">No active spread arbitrage positions.</td></tr>`;
            } else {
                for (const pos of list) {
                    // Fetch live prices to estimate live P&L
                    const ethRes = document.getElementById('price-a').innerText.replace(/[$,]/g, '');
                    const btcRes = document.getElementById('price-b').innerText.replace(/[$,]/g, '');
                    const currentPriceA = parseFloat(ethRes) || pos.entry_price_a;
                    const currentPriceB = parseFloat(btcRes) || pos.entry_price_b;

                    // Leg A PnL
                    const pnlA = pos.direction === "BUY_SPREAD"
                        ? pos.qty_a * (currentPriceA - pos.entry_price_a)
                        : pos.qty_a * (pos.entry_price_a - currentPriceA);

                    // Leg B PnL
                    const pnlB = pos.direction === "BUY_SPREAD"
                        ? pos.qty_b * (pos.entry_price_b - currentPriceB)
                        : pos.qty_b * (currentPriceB - pos.entry_price_b);

                    const fees = pos.deployed_usdt * 0.0016;
                    const estPnl = pnlA + pnlB - fees;

                    const pnlClass = estPnl >= 0 ? 'text-pnl-up' : 'text-pnl-down';
                    
                    const ageMs = new Date() - new Date(pos.opened_at);
                    const ageMin = Math.floor(ageMs / 60000);
                    const ageSec = Math.floor((ageMs % 60000) / 1000);

                    const row = document.createElement('tr');
                    row.innerHTML = `
                        <td style="font-weight: bold;">${pos.pair_name}</td>
                        <td class="${pos.direction === 'BUY_SPREAD' ? 'text-buy' : 'text-sell'}">${pos.direction}</td>
                        <td>${pos.entry_z_score.toFixed(2)}</td>
                        <td>${pos.entry_ratio.toFixed(6)}</td>
                        <td>$${pos.entry_price_a.toFixed(2)} / $${pos.entry_price_b.toFixed(2)}</td>
                        <td>$${currentPriceA.toFixed(2)} / $${currentPriceB.toFixed(2)}</td>
                        <td>${formatUSDT(pos.deployed_usdt)}</td>
                        <td class="${pnlClass}">${formatUSDT(estPnl)}</td>
                        <td>${ageMin}m ${ageSec}s</td>
                    `;
                    tbody.appendChild(row);
                }
            }
        }
    } catch (err) {
        console.error('Error fetching ticks:', err);
    }
}

// Fetch historical data & logs every 5 seconds
async function fetchAnalytics() {
    try {
        // Fetch balance history
        const balRes = await fetch(getApiPrefix() + 'api/balance_history');
        if (balRes.ok) {
            const list = await balRes.json();
            const labels = list.map(item => new Date(item.timestamp).toLocaleTimeString());
            const equities = list.map(item => item.total_equity);

            equityChartObj.data.labels = labels;
            equityChartObj.data.datasets[0].data = equities;
            equityChartObj.update('none');
        }

        // Fetch trading history
        const histRes = await fetch(getApiPrefix() + 'api/history');
        if (histRes.ok) {
            const list = await histRes.json();
            const tbody = document.getElementById('history-body');
            tbody.innerHTML = '';

            if (list.length === 0) {
                tbody.innerHTML = `<tr><td colspan="7" style="text-align: center; color: var(--text-secondary);">No transaction history.</td></tr>`;
            } else {
                for (const row of list) {
                    const ts = new Date(row.timestamp).toLocaleString();
                    const actionClass = row.action.startsWith('OPEN') ? 'text-buy' : 'text-sell';
                    const pnlVal = row.net_pnl !== null ? formatUSDT(row.net_pnl) : '-';
                    const pnlClass = row.net_pnl >= 0 ? 'text-pnl-up' : (row.net_pnl < 0 ? 'text-pnl-down' : '');

                    const tr = document.createElement('tr');
                    tr.innerHTML = `
                        <td>${ts}</td>
                        <td style="font-weight: bold;">${row.pair_name}</td>
                        <td class="${actionClass}">${row.action}</td>
                        <td>$${row.price_a.toFixed(2)} / $${row.price_b.toFixed(2)}</td>
                        <td>${row.z_score.toFixed(2)}</td>
                        <td class="${pnlClass}">${pnlVal}</td>
                        <td>${row.notes || ''}</td>
                    `;
                    tbody.appendChild(tr);
                }
            }
        }

        // Fetch corrections
        const corrRes = await fetch(getApiPrefix() + 'api/corrections');
        if (corrRes.ok) {
            const list = await corrRes.json();
            const tbody = document.getElementById('corrections-body');
            tbody.innerHTML = '';

            if (list.length === 0) {
                tbody.innerHTML = `<tr><td colspan="3" style="text-align: center; color: var(--text-secondary);">No errors logged. System healthy.</td></tr>`;
            } else {
                for (const row of list) {
                    const ts = new Date(row.timestamp).toLocaleString();
                    const tr = document.createElement('tr');
                    tr.innerHTML = `
                        <td>${ts}</td>
                        <td class="text-sell" style="font-weight: bold;">${row.error_type}</td>
                        <td>${row.reason}</td>
                    `;
                    tbody.appendChild(tr);
                }
            }
        }

        // Fetch logs
        const logRes = await fetch(getApiPrefix() + 'api/logs');
        if (logRes.ok) {
            const list = await logRes.json();
            const logBox = document.getElementById('log-viewport');
            const wasAtBottom = logBox.scrollHeight - logBox.clientHeight <= logBox.scrollTop + 20;
            
            logBox.innerText = list.join('\n');
            
            if (wasAtBottom) {
                logBox.scrollTop = logBox.scrollHeight;
            }
        }

    } catch (err) {
        console.error('Error fetching analytics:', err);
    }
}

// Uptime label calculator
function updateUptime() {
    if (!startTime) return;
    const diff = new Date() - startTime;
    const hrs = Math.floor(diff / 3600000).toString().padStart(2, '0');
    const mins = Math.floor((diff % 3600000) / 60000).toString().padStart(2, '0');
    const secs = Math.floor((diff % 60000) / 1000).toString().padStart(2, '0');
    const uptimeEl = document.getElementById('uptime-label');
    if (uptimeEl) uptimeEl.innerText = `${hrs}:${mins}:${secs}`;
}

// Altcoin Co-Integration Scanner State (Pencari Koin Arbitrase 300+ Pairs)
let allScannerPairs = [
    { name: 'SOL / BTC', symbol_a: 'SOLUSDT', symbol_b: 'BTCUSDT', price_a: 142.50, price_b: 62400.0, ratio: 0.002283, mean: 0.002283, std: 0.000045, r2: 98.4, z_score: 0.45, category: 'Layer-1 / L2', est_apr: 24.5 },
    { name: 'BNB / BTC', symbol_a: 'BNBUSDT', symbol_b: 'BTCUSDT', price_a: 585.20, price_b: 62400.0, ratio: 0.009378, mean: 0.009378, std: 0.000120, r2: 97.8, z_score: -1.15, category: 'Layer-1 / L2', est_apr: 18.2 },
    { name: 'LINK / ETH', symbol_a: 'LINKUSDT', symbol_b: 'ETHUSDT', price_a: 14.80, price_b: 3420.0, ratio: 0.004327, mean: 0.004327, std: 0.000085, r2: 96.5, z_score: 2.15, category: 'DeFi / Dex', est_apr: 65.4 },
    { name: 'AVAX / SOL', symbol_a: 'AVAXUSDT', symbol_b: 'SOLUSDT', price_a: 28.40, price_b: 142.50, ratio: 0.199298, mean: 0.199298, std: 0.003500, r2: 95.2, z_score: -2.35, category: 'Layer-1 / L2', est_apr: 78.1 },
    { name: 'ADA / BTC', symbol_a: 'ADAUSDT', symbol_b: 'BTCUSDT', price_a: 0.395, price_b: 62400.0, ratio: 0.00000633, mean: 0.00000633, std: 0.00000015, r2: 94.1, z_score: 0.85, category: 'Layer-1 / L2', est_apr: 32.1 },
    { name: 'DOGE / SHIB', symbol_a: 'DOGEUSDT', symbol_b: 'SHIBUSDT', price_a: 0.124, price_b: 0.0000175, ratio: 7085.71, mean: 7085.71, std: 145.20, r2: 99.1, z_score: -0.65, category: 'Meme Coins', est_apr: 28.9 },
    { name: 'XRP / ADA', symbol_a: 'XRPUSDT', symbol_b: 'ADAUSDT', price_a: 0.585, price_b: 0.395, ratio: 1.4810, mean: 1.4810, std: 0.0280, r2: 93.8, z_score: 1.45, category: 'Layer-1 / L2', est_apr: 48.0 },
    { name: 'NEAR / SOL', symbol_a: 'NEARUSDT', symbol_b: 'SOLUSDT', price_a: 5.40, price_b: 142.50, ratio: 0.03789, mean: 0.03789, std: 0.00095, r2: 95.9, z_score: -1.85, category: 'AI & Big Data', est_apr: 56.3 },
    { name: 'PEPE / FLOKI', symbol_a: 'PEPEUSDT', symbol_b: 'FLOKIUSDT', price_a: 0.0000115, price_b: 0.000175, ratio: 0.06571, mean: 0.06571, std: 0.00180, r2: 98.7, z_score: 2.45, category: 'Meme Coins', est_apr: 88.5 },
    { name: 'ARB / OP', symbol_a: 'ARBUSDT', symbol_b: 'OPUSDT', price_a: 0.785, price_b: 1.850, ratio: 0.4243, mean: 0.4243, std: 0.0085, r2: 97.3, z_score: 0.15, category: 'DeFi / Dex', est_apr: 16.4 }
];

let currentCoinFilter = 'all';
let currentSearchQuery = '';

function filterCoins(category, btnElement) {
    currentCoinFilter = category;
    if (btnElement) {
        const btns = document.querySelectorAll('.filter-btn-coin');
        btns.forEach(b => b.classList.remove('active', 'btn-primary'));
        btnElement.classList.add('active', 'btn-primary');
    }
    renderCoinScanner();
}

function searchCoins(inputElement) {
    currentSearchQuery = inputElement.value.trim().toLowerCase();
    renderCoinScanner();
}

async function updateCoinScanner() {
    try {
        const res = await fetch(getApiPrefix() + 'api/coin_scanner');
        if (res.ok) {
            const data = await res.json();
            if (data && data.length > 0) {
                allScannerPairs = data;
            }
        }
    } catch (err) {
        // Fallback or ignore if offline
    }
    renderCoinScanner();
}

function renderCoinScanner() {
    const tbody = document.getElementById('coin-scanner-body');
    if (!tbody) return;

    // Filter by category and search query
    let filtered = allScannerPairs.filter(p => {
        let matchesCat = true;
        if (currentCoinFilter === 'actionable') matchesCat = Math.abs(p.z_score) > 1.8;
        else if (currentCoinFilter === 'l1') matchesCat = p.category.includes('Layer-1') || p.category.includes('L2');
        else if (currentCoinFilter === 'ai') matchesCat = p.category.includes('AI');
        else if (currentCoinFilter === 'meme') matchesCat = p.category.includes('Meme');
        else if (currentCoinFilter === 'defi') matchesCat = p.category.includes('DeFi') || p.category.includes('Dex');

        let matchesSearch = true;
        if (currentSearchQuery !== '') {
            matchesSearch = p.name.toLowerCase().includes(currentSearchQuery) || 
                            p.symbol_a.toLowerCase().includes(currentSearchQuery) || 
                            p.category.toLowerCase().includes(currentSearchQuery);
        }
        return matchesCat && matchesSearch;
    });

    // Sort by absolute Z-score descending (highest opportunity first)
    filtered.sort((a, b) => Math.abs(b.z_score) - Math.abs(a.z_score));

    if (filtered.length === 0) {
        tbody.innerHTML = `<tr><td colspan="9" style="text-align: center; padding: 30px; color: var(--text-secondary);">🔍 No cointegrated pairs matched your search query or filter criteria.</td></tr>`;
        return;
    }

    tbody.innerHTML = filtered.slice(0, 100).map(p => {
        const ratio = p.price_a / p.price_b;
        let zColor = 'var(--text-primary)';
        let signalHtml = '<span class="badge" style="background: #e5e5e5; color: var(--text-secondary); border: 1px solid var(--border-color);">⏳ NEUTRAL</span>';
        let rowStyle = '';

        if (p.z_score > 2.0) {
            zColor = 'var(--accent-red)';
            signalHtml = '<span class="badge" style="background: var(--accent-red); color: white; font-weight: bold; padding: 4px 8px;">🔥 SELL SPREAD</span>';
            rowStyle = 'background-color: rgba(185, 28, 28, 0.06); font-weight: bold;';
        } else if (p.z_score < -2.0) {
            zColor = 'var(--accent-blue)';
            signalHtml = '<span class="badge" style="background: var(--accent-blue); color: white; font-weight: bold; padding: 4px 8px;">⚡ BUY SPREAD</span>';
            rowStyle = 'background-color: rgba(15, 76, 129, 0.06); font-weight: bold;';
        } else if (Math.abs(p.z_score) > 1.5) {
            signalHtml = '<span class="badge" style="background: #f59e0b; color: white;">👀 WATCHING</span>';
        }

        const formatPrice = (val) => {
            if (val < 0.0001) return '$' + val.toFixed(7);
            if (val < 1) return '$' + val.toFixed(4);
            return '$' + val.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });
        };

        let catBadge = `<span style="font-size: 0.75em; padding: 2px 6px; border-radius: 4px; background: #f3f4f6; color: var(--text-secondary); border: 1px solid var(--border-color);">${p.category}</span>`;

        return `
            <tr style="${rowStyle}">
                <td style="font-weight: bold; font-family: 'Playfair Display', serif;">
                    ${p.name} <br> ${catBadge}
                </td>
                <td style="font-family: monospace;">${formatPrice(p.price_a)} / <br> ${formatPrice(p.price_b)}</td>
                <td style="font-family: monospace;">${ratio.toFixed(6)}</td>
                <td style="font-family: monospace;">${p.mean.toFixed(6)}</td>
                <td style="font-family: monospace;">${p.std.toFixed(6)}</td>
                <td style="font-family: monospace; color: var(--accent-green); font-weight: bold;">${p.r2.toFixed(1)}%</td>
                <td style="font-family: monospace; font-weight: bold; color: var(--accent-blue);">${(p.est_apr || 20.0).toFixed(1)}% APR</td>
                <td style="font-family: monospace; font-size: 1.1em; font-weight: bold; color: ${zColor};">${p.z_score >= 0 ? '+' : ''}${p.z_score.toFixed(2)}&sigma;</td>
                <td>${signalHtml}</td>
            </tr>
        `;
    }).join('');
}

// Main initializer
window.addEventListener('DOMContentLoaded', () => {
    initCharts();

    // Call once
    fetchStatus();
    fetchTickData();
    fetchAnalytics();
    updateCoinScanner();

    // Set loops
    setInterval(fetchStatus, 1000);
    setInterval(fetchTickData, 1000);
    setInterval(fetchAnalytics, 5000);
    setInterval(updateUptime, 1000);
    setInterval(updateCoinScanner, 2000);
});


