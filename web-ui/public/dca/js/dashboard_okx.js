function getApiPrefix() {
    const path = window.location.pathname;
    if (path.startsWith('/alt') || path.indexOf('/alt/') !== -1) {
        return '/alt/';
    } else if (path.startsWith('/okx') || path.indexOf('/okx/') !== -1) {
        return '/okx/';
    } else if (path.startsWith('/dca') || path.indexOf('/dca/') !== -1) {
        return '/dca/';
    }
    return '/';
}
let lastPrice = 0;
        let botStartTime = null;
        let priceChart = null;
        let balanceChart = null;
        let isBalanceHistoryInitialized = false;

        const priceChartEl = document.getElementById('priceChart');
        if (priceChartEl) {
            priceChart = new Chart(priceChartEl.getContext('2d'), {
                type: 'line',
                data: {
                    labels: [],
                    datasets: [{
                        borderColor: '#1a1a1a', // Ink Black
                        borderWidth: 1.8,
                        data: [],
                        fill: false,
                        tension: 0.1,
                        pointRadius: 0
                    }]
                },
                options: {
                    responsive: true,
                    maintainAspectRatio: false,
                    plugins: { legend: { display: false } },
                    scales: {
                        x: { ticks: { color: '#1a1a1a' }, grid: { color: '#e5dfd5' } },
                        y: { ticks: { color: '#1a1a1a' }, grid: { color: '#e5dfd5' } }
                    }
                }
            });
        }

        const balanceChartEl = document.getElementById('balanceChart');
        if (balanceChartEl) {
            balanceChart = new Chart(balanceChartEl.getContext('2d'), {
                type: 'line',
                data: {
                    labels: [],
                    datasets: [{
                        borderColor: '#0f4c81', // Ink Blue
                        borderWidth: 1.8,
                        data: [],
                        fill: false,
                        tension: 0.1,
                        pointRadius: 0
                    }]
                },
                options: {
                    responsive: true,
                    maintainAspectRatio: false,
                    plugins: { legend: { display: false } },
                    scales: {
                        x: { ticks: { color: '#1a1a1a' }, grid: { color: '#e5dfd5' } },
                        y: { ticks: { color: '#1a1a1a' }, grid: { color: '#e5dfd5' } }
                    }
                }
            });
        }

        const fmtUSD = (val) => new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(val);
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

        // Filter Buttons Logic
        document.querySelectorAll('.filter-btn').forEach(btn => {
            btn.addEventListener('click', function() {
                document.querySelectorAll('.filter-btn').forEach(b => b.classList.remove('active'));
                this.classList.add('active');
                // Nanti bisa dipasang trigger fetch grafik ulang di sini sesuai filter
            });
        });

        // Chart Dropdown Logic
        const coinSelector = document.getElementById('chart-coin-selector');
        if (coinSelector) {
            coinSelector.addEventListener('change', function() {
                if (priceChart) {
                    priceChart.data.labels = [];
                    priceChart.data.datasets[0].data = [];
                    priceChart.update();
                }
                lastPrice = 0; // Reset last price for flashing effect
            });
        }


        async function fetchStatus() {
            try {
                const res = await fetch(getApiPrefix() + 'api/status');
                const data = await res.json();

                // Simpan Start Time untuk Uptime
                if (!botStartTime) {
                    botStartTime = new Date(data.start_time);
                }

                // Update Balances
                const usdt = data.simulated_balance;
                
                // BTC
                const btc = data.btc_balance;
                const btcValue = btc * data.current_btc_price;
                
                // ETH
                const eth = data.eth_balance || 0;
                const ethValue = eth * (data.current_eth_price || 0);
                
                // BNB
                const bnb = data.bnb_balance || 0;
                const bnbValue = bnb * (data.current_bnb_price || 0);
                
                // SOL
                const sol = data.sol_balance || 0;
                const solValue = sol * (data.current_sol_price || 0);
                
                // XRP
                const xrp = data.xrp_balance || 0;
                const xrpValue = xrp * (data.current_xrp_price || 0);
                
                const totalEquity = usdt + btcValue + ethValue + bnbValue + solValue + xrpValue;
                
                const usdtEl = document.getElementById('usdt-balance');
                if (usdtEl) usdtEl.innerText = fmtUSD(usdt);
                const totalEquityEl = document.getElementById('total-equity-val');
                if (totalEquityEl) totalEquityEl.innerText = fmtUSD(totalEquity);
                
                // Update Koin
                const btcBalEl = document.getElementById('btc-balance');
                if (btcBalEl) btcBalEl.innerText = btc.toFixed(6);
                const btcValEl = document.getElementById('btc-value-display');
                if (btcValEl) btcValEl.innerText = fmtUSD(btcValue);

                const ethBalEl = document.getElementById('eth-balance');
                if (ethBalEl) ethBalEl.innerText = eth.toFixed(4);
                const ethValEl = document.getElementById('eth-value-display');
                if (ethValEl) ethValEl.innerText = fmtUSD(ethValue);

                const bnbBalEl = document.getElementById('bnb-balance');
                if (bnbBalEl) bnbBalEl.innerText = bnb.toFixed(4);
                const bnbValEl = document.getElementById('bnb-value-display');
                if (bnbValEl) bnbValEl.innerText = fmtUSD(bnbValue);

                const solBalEl = document.getElementById('sol-balance');
                if (solBalEl) solBalEl.innerText = sol.toFixed(2);
                const solValEl = document.getElementById('sol-value-display');
                if (solValEl) solValEl.innerText = fmtUSD(solValue);

                const xrpBalEl = document.getElementById('xrp-balance');
                if (xrpBalEl) xrpBalEl.innerText = xrp.toFixed(2);
                const xrpValEl = document.getElementById('xrp-value-display');
                if (xrpValEl) xrpValEl.innerText = fmtUSD(xrpValue);
                
                // Update Winrate
                const winrateEl = document.getElementById('winrate-counter');
                if (winrateEl) winrateEl.innerText = data.winrate.toFixed(1) + '%';
                
                // Update System Resources
                const cpuEl = document.getElementById('cpu-counter');
                if (cpuEl) cpuEl.innerText = (data.sys_cpu_pct || 0).toFixed(1) + '%';
                const ramEl = document.getElementById('ram-counter');
                if (ramEl) ramEl.innerText = (data.sys_mem_mb || 0).toFixed(0) + ' MB';
                
                const whaleBadge = document.getElementById('whale-badge');
                const whaleStatus = document.getElementById('whale-status');
                if (whaleBadge && whaleStatus) {
                    if (data.whale_detected) {
                        whaleBadge.style.borderColor = 'var(--accent-green)';
                        whaleBadge.style.color = 'var(--accent-green)';
                        whaleBadge.style.boxShadow = '0 0 10px var(--accent-green)';
                        whaleStatus.innerText = 'DETECTED! 🐋';
                    } else {
                        whaleBadge.style.borderColor = '#3b3b3b';
                        whaleBadge.style.color = '#888';
                        whaleBadge.style.boxShadow = 'none';
                        whaleStatus.innerText = 'SLEEPING';
                    }
                }

                // Update Market Regime & Volatility
                const regime = data.market_regime || 'SIDEWAYS';
                const marketRegimeEl = document.getElementById('market-regime');
                if (marketRegimeEl) marketRegimeEl.innerText = regime;
                const marketVolatilityEl = document.getElementById('market-volatility');
                if (marketVolatilityEl) marketVolatilityEl.innerText = fmtUSD(data.market_volatility);

                try {
                    // Update Regime
                    const regimeBtc = document.getElementById('led-regime-btc');
                    const regimeTextBtc = document.getElementById('regime-text-btc');
                    const setRegime = (id, rgm) => {
                        const rEl = document.getElementById('led-regime-' + id);
                        const rTxt = document.getElementById('regime-text-' + id);
                        if(rEl && rTxt) {
                            rEl.className = 'led-light';
                            rTxt.innerText = rgm || 'SIDEWAYS';
                            if (rgm === 'BULLISH') {
                                rEl.classList.add('active-green');
                                rTxt.style.color = 'var(--accent-green)';
                            } else if (rgm === 'SIDEWAYS') {
                                rEl.classList.add('active-yellow');
                                rTxt.style.color = 'var(--accent-yellow)';
                            } else if (rgm === 'BEARISH') {
                                rEl.classList.add('active-red');
                                rTxt.style.color = 'var(--accent-red)';
                            }
                        }
                    };
                    setRegime('btc', data.market_regime);
                    setRegime('eth', data.market_regime_eth);
                    setRegime('bnb', data.market_regime_bnb);
                    setRegime('sol', data.market_regime_sol);
                    setRegime('xrp', data.market_regime_xrp);

                    // Update Pipeline BTC
                    if (data.ws_active) {
                        triggerLED('led-ws-btc', 'active-green');
                        triggerLED('led-ws-eth', 'active-green');
                        triggerLED('led-ws-bnb', 'active-green');
                        triggerLED('led-ws-sol', 'active-green');
                        triggerLED('led-ws-xrp', 'active-green');
                    } else {
                        turnOffLED('led-ws-btc');
                        turnOffLED('led-ws-eth');
                        turnOffLED('led-ws-bnb');
                        turnOffLED('led-ws-sol');
                        turnOffLED('led-ws-xrp');
                    }
                    if (data.conclude_active) triggerLED('led-conclude-btc', 'active-blue'); else turnOffLED('led-conclude-btc');
                    if (data.validate_active) triggerLED('led-validate-btc', 'active-green'); else turnOffLED('led-validate-btc');
                    if (data.executor_active) triggerLED('led-executor-btc', 'active-green'); else turnOffLED('led-executor-btc');
                    
                    // Global Corrector
                    if (data.corrector_active) triggerLED('led-corrector', 'active-red'); else turnOffLED('led-corrector');
                    // AI LED BTC
                    if (data.conclude_active) triggerLED('led-ai-btc', 'active-blue'); else turnOffLED('led-ai-btc');
                } catch(e) {
                    console.error("Error updating LEDs:", e);
                }
                // Profit / Loss (P&L) dari $1000 modal awal
                const pnlAbsolute = totalEquity - 1000.0;
                const pnlPercentage = (pnlAbsolute / 1000.0) * 100.0;
                const pnlEl = document.getElementById('pnl-value');
                if (pnlEl) {
                    const prefix = pnlAbsolute >= 0 ? '+$' : '-$';
                    pnlEl.innerText = `${prefix}${Math.abs(pnlAbsolute).toFixed(2)} (${pnlPercentage.toFixed(2)}%)`;
                    pnlEl.style.color = pnlAbsolute >= 0 ? 'var(--accent-green)' : 'var(--accent-red)';
                }

                // Update Price & Flashing Effect
                const coinSelector = document.getElementById('chart-coin-selector');
                const selectedCoin = coinSelector ? coinSelector.value : 'BTC';
                
                let price = 0;
                if (selectedCoin === 'BTC') price = data.current_btc_price;
                else if (selectedCoin === 'ETH') price = data.current_eth_price;
                else if (selectedCoin === 'BNB') price = data.current_bnb_price;
                else if (selectedCoin === 'SOL') price = data.current_sol_price;
                else if (selectedCoin === 'XRP') price = data.current_xrp_price;

                const priceEl = document.getElementById('btc-price'); // Update the main display
                if (priceEl && price > 0) {
                    priceEl.innerText = fmtUSD(price);
                    
                    if (lastPrice > 0) {
                        if (price > lastPrice) {
                            priceEl.className = "price-val up";
                        } else if (price < lastPrice) {
                            priceEl.className = "price-val down";
                        }
                    }
                    lastPrice = price;

                    // Plot ke chart harga
                    const timeStr = new Date().toLocaleTimeString('id-ID');
                    if (priceChart) {
                        if (priceChart.data.labels.length === 0 || priceChart.data.datasets[0].data[priceChart.data.datasets[0].data.length - 1] !== price) {
                            priceChart.data.labels.push(timeStr);
                            priceChart.data.datasets[0].data.push(price);
                            if (priceChart.data.labels.length > 25) {
                                priceChart.data.labels.shift();
                                priceChart.data.datasets[0].data.shift();
                            }
                            priceChart.update();
                        }
                    }

                    // Plot ke balance chart secara real-time
                    if (balanceChart) {
                        const currentBalance = totalEquity;
                        const balanceData = balanceChart.data.datasets[0].data;
                        const lastPlottedBalance = balanceData[balanceData.length - 1];
                        if (lastPlottedBalance === undefined || Math.abs(lastPlottedBalance - currentBalance) > 0.01) {
                            balanceChart.data.labels.push(timeStr);
                            balanceChart.data.datasets[0].data.push(currentBalance);
                            if (balanceChart.data.labels.length > 100) {
                                balanceChart.data.labels.shift();
                                balanceChart.data.datasets[0].data.shift();
                            }
                            balanceChart.update();
                        }
                    }
                } else if (priceEl) {
                    priceEl.innerText = fmtUSD(price);
                } else {
                    turnOffLED('led-ws-btc');
                }

            } catch (e) {
                turnOffLED('led-ws-btc');
            }
        }

        async function fetchLogsAndSystemState() {
            // LED status diatur secara real-time langsung dari payload status API
        }

        async function fetchHistory() {
            try {
                const res = await fetch(getApiPrefix() + 'api/history');
                const list = await res.json();
                
                let winCount = 0;
                let lossCount = 0;

                const historyBody = document.getElementById('history-body');
                if (historyBody) {
                    historyBody.innerHTML = list.map(item => {
                        const actClass = item.action === 'BUY' ? 'text-buy' : 'text-sell';
                        const time = new Date(item.timestamp).toLocaleTimeString('id-ID');
                        const notesVal = item.notes || '-';
                        let formattedNotes = notesVal;
                        let rowStyle = '';
                        
                        if (notesVal.includes('P&L: $+')) {
                            winCount++;
                            formattedNotes = notesVal.replace(/(P&L:\s*\+\$[0-9.-]+)/g, '<span class="text-buy" style="font-weight: bold;">$1</span>');
                            rowStyle = 'background: rgba(63, 185, 80, 0.05);';
                        } else if (notesVal.includes('P&L: $-')) {
                            lossCount++;
                            formattedNotes = notesVal.replace(/(P&L:\s*-\$[0-9.-]+)/g, '<span class="text-sell" style="font-weight: bold;">$1</span>');
                            rowStyle = 'background: rgba(248, 81, 73, 0.15);'; // Red background for loss
                        }
                        
                        return `
                            <tr style="${rowStyle}">
                                 <td>${time}</td>
                                 <td class="${actClass}">${item.action}</td>
                                 <td>${fmtUSD(item.price)}</td>
                                 <td>${item.amount.toFixed(6)} BTC (${fmtUSD(item.amount * item.price)})</td>
                                 <td>${formattedNotes}</td>
                            </tr>
                        `;
                    }).join('');
                }
                

                
                // Update History Title
                const historyTitle = document.getElementById('history-title');
                if(historyTitle) {
                    historyTitle.innerText = `Riwayat Eksekusi (bot_trading_history) | Wins: ${winCount} | Losses: ${lossCount}`;
                }
            } catch (e) {}
        }
        async function fetchCorrections() {
            try {
                const res = await fetch(getApiPrefix() + 'api/corrections');
                const list = await res.json();
                const correctionsBody = document.getElementById('corrections-body');
                if (correctionsBody) {
                    correctionsBody.innerHTML = list.map(item => {
                        const time = new Date(item.timestamp).toLocaleTimeString('id-ID');
                        return `
                            <tr>
                                <td>${time}</td>
                                <td class="text-fail">${item.error_type}</td>
                                <td>${item.reason}</td>
                            </tr>
                        `;
                    }).join('');
                }
            } catch (e) {}
        }

        async function fetchBalanceHistory() {
            if (isBalanceHistoryInitialized) return;
            try {
                const res = await fetch(getApiPrefix() + 'api/balance_history');
                const list = await res.json();
                
                if (balanceChart) {
                    balanceChart.data.labels = list.map(item => new Date(item.timestamp).toLocaleTimeString('id-ID'));
                    balanceChart.data.datasets[0].data = list.map(item => item.total_value);
                    balanceChart.update();
                    isBalanceHistoryInitialized = true;
                }
            } catch (e) {}
        }

        // Helper LEDs
        function triggerLED(id, activeClass) {
            const el = document.getElementById(id);
            if (el) el.className = "led-light " + activeClass;
        }

        // Helper LEDs
        function turnOffLED(id) {
            const el = document.getElementById(id);
            if (el) el.className = "led-light";
        }


        async function fetchJournal() {
            try {
                const res = await fetch(getApiPrefix() + 'api/journal');
                const text = await res.text();
                const box = document.getElementById('journal-box');
                if (box) box.innerText = text;
            } catch (e) {}
        }

        // Interval
        setInterval(fetchStatus, 1000); // 1 detik harga & status
        setInterval(fetchLogsAndSystemState, 2000); // 2 detik status alur log
        setInterval(fetchHistory, 10000);
        setInterval(fetchCorrections, 10000);
        setInterval(fetchBalanceHistory, 10000);
        setInterval(fetchJournal, 15000);

        // Init
        fetchStatus();
        fetchLogsAndSystemState();
        fetchHistory();
        fetchCorrections();
        fetchBalanceHistory();
        fetchJournal();

        // Highlight active navigation link in sidebar
        const currentPath = window.location.pathname;
        document.querySelectorAll('.sidebar-link').forEach(link => {
            if (link.getAttribute('href') === currentPath || (currentPath === '/' && link.getAttribute('href') === '/')) {
                link.classList.add('active');
            } else {
                link.classList.remove('active');
            }
        });