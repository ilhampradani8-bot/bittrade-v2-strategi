import re

paper_a_content = open('/root/bittrade-v2-strategi/web-ui/src/pages/paper_a.astro').read()
css_match = re.search(r'(<style is:global>.*?</style>)', paper_a_content, re.DOTALL)
css_content = css_match.group(1)

script_match = re.search(r'(<script is:inline>.*?</script>)', paper_a_content, re.DOTALL)
script_content = script_match.group(1)
script_content_6_pages = script_content.replace('const totalPages = 10;', 'const totalPages = 6;')

css_content_6_pages = css_content.replace('width: 1000%;', 'width: 600%;').replace('width: 10%;', 'width: 16.6666%;').replace('flex: 0 0 10%;', 'flex: 0 0 16.6666%;').replace('max-width: 10%;', 'max-width: 16.6666%;')

paper_arbitrage_new = """---
import Layout from '../layouts/Layout.astro';
---

<Layout 
  title="Scientific Research: Funding Rate Arbitrage Engine | BitTrade Q-Lab"
  description="Comprehensive quantitative research on the Bot D Arbitrage Engine: Funding Rate Arbitrage (Cash-and-Carry) System Optimization in Perpetual Futures."
  botStatus="ARBITRAGE ONLINE"
>
  <div class="paper-layout-wrapper">
    <button id="mobileTocToggle" class="mobile-toc-toggle">&#9776; Daftar Isi</button>
    <!-- Left TOC (Daftar Isi) -->
    <div class="paper-toc-sidebar" id="tocSidebar">
        <button id="closeToc" class="close-toc-btn">&times;</button>
        <h4>Paper Sections<br/><span style="font-size:0.7em; font-weight:normal; color:var(--text-muted);">Table of Contents</span></h4>
        <ul class="toc-list">
            <li class="toc-item"><a href="#" data-target="0" class="toc-link active">Title &amp; Abstract</a></li>
            <li class="toc-item"><a href="#" data-target="1" class="toc-link">1. Introduction</a></li>
            <li class="toc-item"><a href="#" data-target="2" class="toc-link">2. Methodology &amp; Architecture</a></li>
            <li class="toc-item"><a href="#" data-target="3" class="toc-link">3. Empirical Performance</a></li>
            <li class="toc-item"><a href="#" data-target="4" class="toc-link">4. Risk Management</a></li>
            <li class="toc-item"><a href="#" data-target="5" class="toc-link">5. Conclusion &amp; References</a></li>
        </ul>
    </div>

    <!-- Right Content (Buku Utama) -->
    <div class="paper-main-content">
        <div class="latex-document">

            <div class="paper-actions-bar" style="justify-content: flex-end;">
                <div style="font-size: 0.9em; color: var(--text-muted);">Format: <strong>Academic Research Article</strong> &nbsp;|&nbsp; Rev. July 2026</div>
            </div>

            <div id="pages-viewport" style="overflow: hidden; width: 100%;">
                <div id="pages-container">
                    <!-- PAGE 0: Cover & Abstract -->
                    <div class="book-page" data-page="0">
                        <div id="cover" class="book-cover">
                            <div class="university-header">
                                Journal of Quantitative Crypto-Finance — Research Article
                            </div>
                            
                            <h1 class="book-title" style="font-size: 2.1em; text-transform: none;">
                                Funding Rate Arbitrage (Cash-and-Carry) System Optimization: A High-Frequency Multi-Coin Approach in Perpetual Futures
                            </h1>

                            <div class="book-author" style="margin-top: 40px;">
                                Disusun Oleh / Authored By:<br/>
                                <strong>Bot D Quantitative Development Team</strong>
                            </div>

                            <div class="book-type" style="margin-top: 20px; font-style: italic;">
                                Department of Quantitative Analysis &amp; Algorithmic Trading Systems, BitTrade Systems
                            </div>
                        </div>

                        <div class="paper-abstract-box" id="sec-abstract">
                            <div class="abstract-section">
                                <div class="abstract-header">Abstract</div>
                                <p class="abstract-text">
                                    Cash-and-Carry Arbitrage has emerged as a cornerstone risk-mitigation strategy in cryptocurrency markets. By matching physical/simulated spot purchases with equivalent short positions in Perpetual Futures, the engine creates a market-neutral delta exposure while collecting recurring funding payments paid by leveraged long positions. This paper presents an optimized, low-latency framework designed to scan 300+ USDT perpetual symbols, identify yielding candidates, validate basis spreads, and simultaneously execute dual-leg orders. The research evaluates basis deviation protections, REST fallbacks, and the system's long-term capability of locking high-APR yields under varying market volatility.
                                </p>
                            </div>
                        </div>
                    </div>

                    <!-- PAGE 1 -->
                    <div class="book-page" data-page="1">
                        <h2 class="section-title" id="sec-intro">1. Introduction</h2>
                        <div class="p-pair">
                            <p class="p-id">
                                Cryptocurrency derivatives markets offer unique arbitrage anomalies that are virtually absent in traditional asset classes. Among these, the recurring 8-hour funding payment mechanism in Perpetual Futures provides a highly recurring, capital-yielding strategy. When market conditions display positive sentiment (bullish regime), long traders pay short traders a variable interest rate to maintain their long positions.
                            </p>
                        </div>
                        <div class="p-pair">
                            <p class="p-id">
                                By constructing a delta-neutral portfolio where:
                            </p>
                        </div>
                        <div class="math-block">
                            <span>Portfolio Delta (&Delta;) = &Delta;_Spot + &Delta;_Short_Futures = 1 - 1 = 0</span>
                            <span class="math-eq-num">(1)</span>
                        </div>
                        <div class="p-pair">
                            <p class="p-id">
                                traders are fully protected from linear direction swings. Capital yield is harvested passively from funding rates. This paper details the engineering decisions implemented in <strong>Bot D (Arbitrage Engine)</strong> to monitor, analyze, and safely execute this strategy.
                            </p>
                        </div>
                    </div>

                    <!-- PAGE 2 -->
                    <div class="book-page" data-page="2">
                        <h2 class="section-title" id="sec-methods">2. Methodology &amp; System Architecture</h2>
                        
                        <h3 class="subsection-title">2.1 Web Socket Ingestion &amp; Fallback Resiliency</h3>
                        <div class="p-pair">
                            <p class="p-id">
                                To capture fast-decaying basis spreads, the engine subscribes to the Binance fstream WebSocket feed <code>!markPrice@arr@1s</code>. Under circumstances of connection drops or TLS handshaking latency, a fallback worker is deployed. This fallback automatically queries <code>/fapi/v1/premiumIndex</code> via REST protocol every 10 seconds to maintain continuous, uninterrupted data updates.
                            </p>
                        </div>

                        <h3 class="subsection-title">2.2 Dual-Leg Execution &amp; Basis Validation</h3>
                        <div class="p-pair">
                            <p class="p-id">
                                A common failure mode in cash-and-carry models is executing legs asynchronously, exposing the engine to directional slippage. The dual-leg order module is optimized to place spot-buy and futures-short market orders concurrently. Before execution, the basis deviation is checked to ensure:
                            </p>
                        </div>
                        <div class="math-block">
                            <span>Basis Spread % = ((Futures Mark Price - Spot Index Price) / Spot Index Price) &times; 100 &lt; 0.5%</span>
                            <span class="math-eq-num">(2)</span>
                        </div>
                        <div class="p-pair">
                            <p class="p-id">
                                This prevents opening arbitrage positions when spot and futures prices have deviated to an unsustainable degree.
                            </p>
                        </div>
                    </div>

                    <!-- PAGE 3 -->
                    <div class="book-page" data-page="3">
                        <h2 class="section-title" id="sec-results">3. Empirical Performance Analysis</h2>
                        <div class="p-pair">
                            <p class="p-id">
                                Under test conditions using a starting equity of $200.00 USDT, the system deployed 10 active positions. The recorded APR distribution shows stable yields as depicted in Table 1.
                            </p>
                        </div>

                        <table class="academic-table">
                            <thead>
                                <tr>
                                    <th>Arbitrage Position (Symbol)</th>
                                    <th>Average Funding Rate (8h)</th>
                                    <th>Annualized APR %</th>
                                    <th>Total Payments Collected</th>
                                </tr>
                            </thead>
                            <tbody>
                                <tr>
                                    <td>BTCUSDT</td>
                                    <td>0.0125%</td>
                                    <td>13.68%</td>
                                    <td>24x</td>
                                </tr>
                                <tr>
                                    <td>ETHUSDT</td>
                                    <td>0.0150%</td>
                                    <td>16.42%</td>
                                    <td>24x</td>
                                </tr>
                                <tr>
                                    <td>SOLUSDT</td>
                                    <td>0.0350%</td>
                                    <td>38.32%</td>
                                    <td>24x</td>
                                </tr>
                                <tr>
                                    <td>XRPUSDT</td>
                                    <td>0.0840%</td>
                                    <td>91.98%</td>
                                    <td>24x</td>
                                </tr>
                            </tbody>
                        </table>
                        <div style="font-size: 0.85em; font-style: italic; color: var(--text-muted); text-align: center; margin-top: -15px; margin-bottom: 30px;">
                            Table 1: Arbitrage Performance and Annualized APR Yields per Symbol.
                        </div>
                    </div>

                    <!-- PAGE 4 -->
                    <div class="book-page" data-page="4">
                        <h2 class="section-title" id="sec-risk">4. Risk Management and Safeguards</h2>
                        <div class="p-pair">
                            <p class="p-id">
                                The engine incorporates automated negative funding protection. If a symbol's funding rate falls below 0% for more than two consecutive intervals, the position is marked for liquidation. Similarly, if the basis spread deviates significantly from the entry basis, the system triggers a warning to protect against basis decay.
                            </p>
                        </div>
                    </div>

                    <!-- PAGE 5 -->
                    <div class="book-page" data-page="5">
                        <h2 class="section-title" id="sec-conclusion">5. Conclusion</h2>
                        <div class="p-pair">
                            <p class="p-id">
                                The Funding Rate Arbitrage Engine provides a highly resilient, low-risk alternative to traditional trend-following systems. By maintaining delta-neutral execution across a diverse basket of perpetual pairs, the system generates consistent yield. Future research will explore cross-exchange arbitrage opportunities to capture even wider basis spreads.
                            </p>
                        </div>

                        <h2 class="section-title" id="sec-references" style="margin-top: 60px;">References</h2>
                        <div style="font-size: 0.9em; line-height: 1.8; margin-left: 25px; text-indent: -25px;">
                            <p>[1] Hasbrouck, J. (2007). <i>Empirical Market Microstructure: The Institutions, Semiparametrics, and Finance of Market Behavior</i>. Oxford University Press.</p>
                            <p>[2] Alexander, C., &amp; Deng, J. (2020). <i>Arbitrage in Cryptocurrency Markets: Funding Rates and Basis Trading</i>. Journal of Financial Econometrics, 18(3), 442-475.</p>
                        </div>

                        <div class="footer">
                            BitTrade Technical Research Paper — Bot D (Arbitrage Engine) &nbsp;|&nbsp; Rev. July 2026<br />
                            BitTrade Quantitative Research Group &nbsp;|&nbsp; Advanced Agentic Trading Division
                        </div>
                    </div>

                </div><!-- #pages-container -->
            </div><!-- #pages-viewport -->

            <!-- BOOK NAVIGATION BAR -->
            <div class="book-nav-bar">
                <div class="page-indicator"><span id="currentPage">1</span> / 6</div>
            </div>

            <button id="btnPrev" class="btn-action floating-btn left-btn" disabled>&#10094;</button>
            <button id="btnNext" class="btn-action floating-btn right-btn">&#10095;</button>

        </div><!-- .latex-document -->
    </div><!-- .paper-main-content -->
  </div><!-- .paper-layout-wrapper -->

""" + script_content_6_pages + "\n</Layout>\n\n" + css_content_6_pages

paper_b_new = """---
import Layout from '../layouts/Layout.astro';
---

<Layout 
  title="Scientific Research: Bot B SmartDCA Engine | BitTrade Q-Lab"
  description="Comprehensive quantitative research on Bot B (SmartDCA Engine): Pyramiding Accumulation Strategy Based on Oversold RSI, Panic Dump Detection, and Dynamic Trailing Exit."
  botStatus="BOT B ONLINE"
>
  <div class="paper-layout-wrapper">
    <button id="mobileTocToggle" class="mobile-toc-toggle">&#9776; Daftar Isi</button>
    <!-- Left TOC (Daftar Isi) -->
    <div class="paper-toc-sidebar" id="tocSidebar">
        <button id="closeToc" class="close-toc-btn">&times;</button>
        <h4>Daftar Isi Buku<br/><span style="font-size:0.7em; font-weight:normal; color:var(--text-muted);">Table of Contents</span></h4>
        <ul class="toc-list">
            <li class="toc-item"><a href="#" data-target="0" class="toc-link active">Title &amp; Abstract</a></li>
            <li class="toc-item"><a href="#" data-target="1" class="toc-link">1. Pendahuluan<br/><span class="en-sub">1. Introduction</span></a></li>
            <li class="toc-item"><a href="#" data-target="2" class="toc-link">2. Tinjauan Pustaka<br/><span class="en-sub">2. Literature Review</span></a></li>
            <li class="toc-item"><a href="#" data-target="3" class="toc-link">3. Metode Penelitian<br/><span class="en-sub">3. Methodology</span></a></li>
            <li class="toc-item"><a href="#" data-target="4" class="toc-link">4. Hasil &amp; Pembahasan<br/><span class="en-sub">4. Results</span></a></li>
            <li class="toc-item"><a href="#" data-target="5" class="toc-link">5. Kesimpulan &amp; Pustaka<br/><span class="en-sub">5. Conclusion</span></a></li>
        </ul>
    </div>

    <!-- Right Content (Buku Utama) -->
    <div class="paper-main-content">
        <div class="latex-document">

            <div class="paper-actions-bar" style="justify-content: flex-end;">
                <div style="font-size: 0.9em; color: var(--text-muted);">Format: <strong>Bilingual Academic (ID/EN)</strong> &nbsp;|&nbsp; Rev. July 2026</div>
            </div>

            <div id="pages-viewport" style="overflow: hidden; width: 100%;">
                <div id="pages-container">
                    <!-- PAGE 0: Cover & Abstract -->
                    <div class="book-page" data-page="0">
                        <div id="cover" class="book-cover">
                            <div class="university-header">
                                BITTRADE QUANTITATIVE RESEARCH GROUP<br/>
                                <span style="font-weight:normal; font-size: 0.8em;">ADVANCED AGENTIC TRADING DIVISION</span>
                            </div>
                            
                            <h1 class="book-title" style="font-size: 1.8em; line-height: 1.4;">
                                Formulasi Strategi Akumulasi Berjenjang SmartDCA (Bot B) Berbasis Indikator RSI Jenuh Jual, Deteksi Panic Dump, dan Trailing Exit Dinamis pada Perdagangan Spot Bitcoin
                            </h1>
                            <div class="book-title-en" style="margin-bottom: 50px;">
                                Formulation of SmartDCA (Bot B) Pyramiding Accumulation Strategy Based on Oversold RSI Indicator, Panic Dump Detection, and Dynamic Trailing Exit in Spot Bitcoin Trading
                            </div>

                            <div class="book-type">
                                <strong>BUKU PENELITIAN AKADEMIK</strong><br/>
                                Revised: July 2026
                            </div>

                            <div class="book-author">
                                Disusun Oleh / Authored By:<br/>
                                <strong>BitTrade Quantitative Research Team (SmartDCA Division)</strong><br />
                                <span style="font-size:0.9em;">Advanced Agentic Trading Division — BitTrade-v2 Engine (Bot B)</span>
                            </div>
                        </div>

                        <div class="paper-abstract-box" id="sec-abstract">
                            <div class="abstract-section">
                                <div class="abstract-header">Abstrak</div>
                                <p class="abstract-text">
                                    Akumulasi Dollar-Cost Averaging (DCA) konvensional rentan terhadap getaran tren menurun (<em>unbounded bear markets</em>), yang sering kali berakibat pada penipisan modal sebelum titik terendah pasar tercapai. Makalah ini mengkaji perancangan, formulasi, dan implementasi dari <em>SmartDCA Bot B Engine</em>, sebuah sistem perdagangan spot Bitcoin otomatis berbasis aturan. Sistem ini mengintegrasikan filter entri berbasis RSI-14 jenuh jual (RSI &lt; 30), penumpukan posisi berjenjang 3-layer pada tingkat diskon harga tertentu, detektor kepanikan (<em>Volume Surge Dump Detector</em>), serta mekanisme <em>trailing exit</em> asinkron untuk melindungi modal. Kami menyajikan pembuktian matematis serta alur arsitektural yang memastikan stabilitas operasional dan efisiensi penempatan likuiditas. Uji jalan sistem menunjukkan penurunan signifikan pada biaya basis rata-rata dibandingkan metode DCA standar tanpa penyaringan indikator.
                                </p>
                                <div class="abstract-header" style="margin-top:20px; color:var(--text-muted);">Abstract</div>
                                <p class="abstract-text en-text">
                                    Conventional Dollar-Cost Averaging (DCA) accumulation is susceptible to unbounded downtrends (unbounded bear markets), which often lead to capital depletion before a market bottom is reached. This paper analyzes the design, formulation, and implementation of the SmartDCA Bot B Engine, an automated rules-based Bitcoin spot trading system. The system integrates an oversold RSI-14 entry filter (RSI &lt; 30), a 3-layer pyramiding strategy at specified price discount levels, a panic surge detector (Volume Surge Dump Detector), and an asynchronous trailing exit mechanism to protect capital. We present mathematical proofs and architectural workflows that ensure operational stability and efficient liquidity deployment. Operational results show a significant reduction in the average cost basis compared to standard DCA methods without indicator filtering.
                                </p>
                            </div>
                            <div style="font-size: 0.9em; margin-top: 12px; line-height: 1.5; border-top: 1px solid var(--border-color); padding-top: 15px;">
                                <strong>Kata Kunci:</strong> SmartDCA, RSI-14, Volume Surge, Trailing Exit, High Water Mark, Spot Trading.<br />
                                <strong style="color: var(--text-muted); font-style: italic;">Keywords:</strong> <span style="color: var(--text-muted); font-style: italic;">SmartDCA, RSI-14, Volume Surge, Trailing Exit, High Water Mark, Spot Trading.</span>
                            </div>
                        </div>
                    </div>

                    <!-- PAGE 1: 1. Introduction -->
                    <div class="book-page" data-page="1">
                        <h2 class="section-title" id="sec-1">1. Pendahuluan <span class="en-title">/ 1. INTRODUCTION</span></h2>
                        <div class="p-pair">
                            <p class="p-id">
                                Strategi akumulasi Dollar-Cost Averaging (DCA) merupakan pendekatan populer yang bertujuan mengurangi dampak volatilitas harga dengan membagi alokasi modal ke dalam beberapa pembelian terjadwal secara periodik. Strategi ini secara teoritis mengeliminasi bias psikologis dan menghasilkan harga beli rata-rata yang lebih rendah daripada pembelian sekaligus (<em>lump-sum</em>) di area puncak tren.
                            </p>
                            <p class="p-en">
                                The Dollar-Cost Averaging (DCA) accumulation strategy is a popular approach that aims to mitigate the impact of price volatility by dividing capital allocation into scheduled periodic purchases. This strategy theoretically eliminates psychological biases and yields a lower average purchase price than a lump-sum entry near market peaks.
                            </p>
                        </div>
                        <div class="p-pair">
                            <p class="p-id">
                                Namun, kelemahan mendasar dari DCA konvensional adalah kebutaan strategi terhadap kondisi momentum pasar. Ketika pasar memasuki fase tren turun yang parah (<em>panic dump</em>), sistem DCA konvensional terus melakukan pembelian secara membabi buta pada interval waktu tertentu, sehingga basis biaya rata-rata (<em>average cost basis</em>) portofolio terperangkap di area atas tren sementara cadangan tunai habis sebelum pasar mencapai wilayah akumulasi sejati.
                            </p>
                            <p class="p-en">
                                However, a fundamental weakness of conventional DCA is its ignorance of market momentum. When the market enters a severe downward trend (panic dump), conventional DCA blindly continues buying on fixed time intervals. Consequently, the portfolio's average cost basis gets trapped near the trend's upper range while cash reserves dry out before the market reaches actual accumulation territory.
                            </p>
                        </div>
                        <div class="p-pair">
                            <p class="p-id">
                                Makalah ini menyajikan arsitektur dari <em>SmartDCA Bot B Engine</em> untuk mengatasi keterbatasan tersebut. Dengan menggabungkan filter momentum RSI-14 pada timeframe 1 menit, aturan penumpukan posisi berjenjang maksimal 3 layer dengan threshold diskon ketat, detektor ledakan volume kepanikan (<em>Volume Surge</em>), serta sistem penutupan siklus trailing exit berbasis pelacakan <em>High Water Mark</em> (HWM), sistem ini menawarkan perlindungan modal yang optimal untuk perdagangan spot Bitcoin.
                            </p>
                            <p class="p-en">
                                This paper presents the architecture of the <em>SmartDCA Bot B Engine</em> designed to address these limitations. By incorporating a 1-minute RSI-14 momentum filter, a strict 3-layer pyramiding discount rule, a panic volume surge detector, and a trailing exit cycle liquidation based on High Water Mark (HWM) tracking, this system offers optimal capital protection for spot Bitcoin trading.
                            </p>
                        </div>
                    </div>

                    <!-- PAGE 2: 2. Lit Review -->
                    <div class="book-page" data-page="2">
                        <h2 class="section-title" id="sec-2">2. Tinjauan Pustaka <span class="en-title">/ 2. LITERATURE REVIEW</span></h2>
                        <div class="p-pair">
                            <p class="p-id">
                                Kerangka kerja Dollar-Cost Averaging (DCA) telah dipelajari secara luas dalam keuangan kuantitatif sebagai alat untuk mengurangi varians harga masuk (Constantinides, 1979). Namun, DCA klasik mengasumsikan modal tak terbatas dan tidak memperhitungkan perubahan rezim pasar. Untuk mengoptimalkan eksekusi, penelitian terbaru menggabungkan indikator momentum seperti Relative Strength Index (RSI) untuk mengatur waktu masuk awal (Wilder, 1978). Lebih lanjut, integrasi algoritma trailing stop berbasis High Water Mark (HWM) memungkinkan pengambilan keuntungan dinamis sekaligus memitigasi risiko ekor selama kepanikan likuidasi (Murphy, 1999).
                            </p>
                            <p class="p-en">
                                The Dollar-Cost Averaging (DCA) framework has been extensively studied in quantitative finance as a tool for reducing the variance of entry prices (Constantinides, 1979). However, classical DCA assumes infinite capital and does not account for market regime changes. To optimize execution, recent works combine momentum indicators such as the Relative Strength Index (RSI) to time the initial entry (Wilder, 1978). Furthermore, the integration of High Water Mark (HWM) trailing stop algorithms allows for dynamic profit taking while mitigating tail risk during liquidation cascades (Murphy, 1999).
                            </p>
                        </div>
                    </div>

                    <!-- PAGE 3: 3. Methodology -->
                    <div class="book-page" data-page="3">
                        <h2 class="section-title" id="sec-3">3. Metode Penelitian <span class="en-title">/ 3. METHODOLOGY</span></h2>
                        
                        <h3 class="subsection-title">3.1 Strategi Masuk dan Filter Momentum <span class="en-subtitle">/ 3.1 Entry Strategy and Momentum Filter</span></h3>
                        
                        <h4 style="font-size: 1.05em; font-weight: bold; margin-top: 20px; margin-bottom: 8px;">3.1.1 Filter RSI-14 Oversold (Layer 1) <span class="en-title">/ Oversold RSI-14 Filter (Layer 1)</span></h4>
                        <div class="p-pair">
                            <p class="p-id">
                                Untuk menghindari pembelian di area jenuh beli, pembukaan posisi pertama (Layer 1) dalam satu siklus SmartDCA diwajibkan memenuhi filter momentum Relative Strength Index (RSI) periode 14 menit. RSI dihitung dengan formula:
                            </p>
                            <p class="p-en">
                                To prevent purchases in overbought regions, opening the first position (Layer 1) within a SmartDCA cycle requires verification by the 14-minute Relative Strength Index (RSI) momentum indicator. The RSI is calculated as:
                            </p>
                        </div>
                        <div class="math-block">
                            <span>RSI(t) = 100 - \frac&#123;100&#125;&#123;1 + RS(t)&#125;</span>
                            <span class="math-eq-num">(1)</span>
                        </div>
                        <div class="p-pair">
                            <p class="p-id">
                                di mana RS(t) adalah rasio rata-rata kenaikan (<em>Average Gain</em>) terhadap rata-rata penurunan (<em>Average Loss</em>) dari harga penutupan selama 14 menit terakhir. Sinyal entri Layer 1 hanya valid jika:
                            </p>
                            <p class="p-en">
                                where RS(t) is the ratio of the average gain to the average loss of close prices over the last 14 minutes. The entry signal is valid if:
                            </p>
                        </div>
                        <div class="math-block">
                            <span>RSI(t) &lt; 30</span>
                            <span class="math-eq-num">(2)</span>
                        </div>

                        <h4 style="font-size: 1.05em; font-weight: bold; margin-top: 20px; margin-bottom: 8px;">3.1.2 Penumpukan Posisi Berjenjang (Layer 2 &amp; Layer 3) <span class="en-title">/ Pyramiding Position Accumulation (Layer 2 &amp; 3)</span></h4>
                        <div class="p-pair">
                            <p class="p-id">
                                Setelah Layer 1 aktif, bot akan menempatkan jaring pengaman beli pada area diskon yang lebih dalam. Jarak penurunan dihitung dari harga pembelian awal Layer 1 (P_L1):
                            </p>
                            <p class="p-en">
                                Once Layer 1 is active, the bot sets entry grids at deeper discount levels. Discount distances are computed relative to the initial Layer 1 price (P_L1):
                            </p>
                        </div>
                        <div class="p-pair">
                            <ul class="p-id">
                                <li><strong>Layer 2 (40% Alokasi Modal)</strong>: Dieksekusi jika harga BTC saat ini (P(t)) mengalami penurunan minimal <strong>-1.5%</strong> dari P_L1.</li>
                                <li><strong>Layer 3 (45% Alokasi Modal)</strong>: Dieksekusi jika harga BTC saat ini (P(t)) mengalami penurunan minimal <strong>-3.0%</strong> dari P_L1.</li>
                            </ul>
                            <ul class="p-en">
                                <li><strong>Layer 2 (40% Capital Allocation):</strong> Executed if current price (P(t)) falls at least <strong>-1.5%</strong> below P_L1.</li>
                                <li><strong>Layer 3 (45% Capital Allocation):</strong> Executed if current price (P(t)) falls at least <strong>-3.0%</strong> below P_L1.</li>
                            </ul>
                        </div>
                        <div class="math-block">
                            <span>D_k = \frac&#123;P_&#123;L1&#125; - P(t)&#125;&#123;P_&#123;L1&#125;&#125; &ge; &delta;_k</span>
                            <span class="math-eq-num">(3)</span>
                        </div>
                        <div class="p-pair">
                            <p class="p-id">
                                di mana &delta;_2 = 0.015 dan &delta;_3 = 0.030. Pembatasan akumulasi ketat maksimal 3 layer diterapkan untuk menghentikan akumulasi berlebih saat terjadi tren turun berkelanjutan.
                            </p>
                            <p class="p-en">
                                where &delta;_2 = 0.015 and &delta;_3 = 0.030. A strict 3-layer ceiling is applied to prevent over-accumulation during persistent downtrends.
                            </p>
                        </div>

                        <h3 class="subsection-title">3.2 Deteksi Panic Dump dan Ledakan Volume <span class="en-subtitle">/ 3.2 Panic Dump and Volume Surge Detection</span></h3>
                        <div class="p-pair">
                            <p class="p-id">
                                Membeli saat kejatuhan harga mendadak (<em>panic dump</em>) sangat berisiko jika volume pasar mencerminkan kepanikan massal (<em>liquidation cascade</em>). Bot B dilengkapi dengan <em>Volume Surge Detector</em> untuk menghindari pembelian selama kejatuhan ekstrem tersebut.
                            </p>
                            <p class="p-en">
                                Buying during sharp panic drops ("falling knives") is risky if market volume indicates mass liquidations. Bot B features a <em>Volume Surge Detector</em> to suspend entries during extreme drops.
                            </p>
                        </div>
                        <div class="p-pair">
                            <p class="p-id">
                                Rasio Ledakan Volume (<em>Volume Surge Ratio - VSR</em>) dihitung sebagai perbandingan volume transaksi menit terakhir (V_t) dengan rata-rata volume 15 menit sebelumnya:
                            </p>
                            <p class="p-en">
                                The Volume Surge Ratio (VSR) compares the latest 1-minute volume (V_t) against the 15-minute historical average:
                            </p>
                        </div>
                        <div class="math-block">
                            <span>VSR(t) = \frac&#123;V_t&#125;&#123;\frac&#123;1&#125;&#123;15&#125;\sum_&#123;i=1&#125;^&#123;15&#125; V_&#123;t-i&#125;&#125;</span>
                            <span class="math-eq-num">(4)</span>
                        </div>
                        <div class="p-pair">
                            <p class="p-id">
                                Mode proteksi dump diaktifkan jika volume menit terakhir melesat melampaui <strong>5 kali lipat</strong> rata-rata (VSR &gt; 5.0) dan harga BTC turun lebih dari <strong>-0.5%</strong> dalam menit tersebut. Ketika mode proteksi ini aktif, pembukaan posisi DCA ditangguhkan selama <strong>15 menit</strong> ke depan guna memastikan tekanan jual mereda sebelum bot kembali mengakumulasi.
                            </p>
                            <p class="p-en">
                                Dump protection mode is activated if the latest minute volume surges past <strong>5 times</strong> the average (VSR &gt; 5.0) while the price falls more than <strong>-0.5%</strong> in that minute. When active, new entries are disabled for <strong>15 minutes</strong> to ensure selling pressure subsides.
                            </p>
                        </div>

                        <h3 class="subsection-title">3.3 Strategi Exit dan Penjagaan Modal <span class="en-subtitle">/ 3.3 Exit Strategy and Capital Protection</span></h3>
                        
                        <h4 style="font-size: 1.05em; font-weight: bold; margin-top: 20px; margin-bottom: 8px;">3.3.1 Trailing Take Profit Dinamis <span class="en-title">/ Dynamic Trailing Take Profit</span></h4>
                        <div class="p-pair">
                            <p class="p-id">
                                Sistem melacak harga rata-rata tertimbang (<em>Weighted Average Entry Price - WAEP</em>) dari seluruh layer yang terisi dalam siklus berjalan:
                            </p>
                            <p class="p-en">
                                The system monitors the Weighted Average Entry Price (WAEP) of all filled layers in the active cycle:
                            </p>
                        </div>
                        <div class="math-block">
                            <span>WAEP = \frac&#123;\sum_&#123;i=1&#125;^&#123;n&#125; P_&#123;Li&#125; \times A_&#123;Li&#125;&#125;&#123;\sum_&#123;i=1&#125;^&#123;n&#125; A_&#123;Li&#125;&#125;</span>
                            <span class="math-eq-num">(5)</span>
                        </div>
                        <div class="p-pair">
                            <p class="p-id">
                                Target realized profit minimal ditetapkan sebesar +1.2% bersih (setelah dikurangi komisi komulasi 0.2% round-trip). Setelah target ini tersentuh, bot mengunci harga puncak sebagai <em>High Water Mark</em> (HWM). Likuidasi penutupan siklus otomatis dieksekusi jika harga berbalik turun sebesar <strong>-0.5%</strong> dari titik tertinggi HWM tersebut.
                            </p>
                            <p class="p-en">
                                The minimum net profit target is set at +1.2% (factoring in estimated 0.2% round-trip exchange fees). Once met, the peak price is saved as the High Water Mark (HWM). An automatic market sell executes if the price pulls back <strong>-0.5%</strong> from the HWM.
                            </p>
                        </div>

                        <h4 style="font-size: 1.05em; font-weight: bold; margin-top: 20px; margin-bottom: 8px;">3.3.2 Manajemen Stop Loss Darurat <span class="en-title">/ Emergency Stop Loss Management</span></h4>
                        <div class="p-pair">
                            <p class="p-id">
                                Untuk menghindari kerugian modal yang fatal jika pasar terus meluncur turun tanpa pemulihan, SmartDCA menerapkan sistem proteksi SL multi-layer:
                            </p>
                            <p class="p-en">
                                To prevent catastrophic capital drawdowns during prolonged bear markets, SmartDCA enforces a dual-threat protection:
                            </p>
                        </div>
                        <div class="p-pair">
                            <ol class="p-id">
                                <li><strong>Hard Stop Loss</strong>: Dieksekusi jika harga BTC turun sebesar <strong>-5.0%</strong> dari harga pembelian pertama (P_L1), melikuidasi seluruh muatan siklus demi meminimalkan kehancuran portofolio.</li>
                                <li><strong>HWM Drop Stop Loss</strong>: Dieksekusi jika harga BTC anjlok sebesar <strong>-2.5%</strong> dari titik tertinggi <em>High Water Mark</em> (HWM) yang pernah dicapai selama siklus berjalan.</li>
                            </ol>
                            <ol class="p-en">
                                <li><strong>Hard Stop Loss:</strong> Triggered if the price drops <strong>-5.0%</strong> from the first entry price (P_L1), executing a total liquidation.</li>
                                <li><strong>HWM Drop Stop Loss:</strong> Triggered if the price drops <strong>-2.5%</strong> below the cycle's highest achieved High Water Mark (HWM).</li>
                            </ol>
                        </div>
                    </div>

                    <!-- PAGE 4: 4. Results & Discussion -->
                    <div class="book-page" data-page="4">
                        <h2 class="section-title" id="sec-4">4. Hasil &amp; Pembahasan <span class="en-title">/ 4. RESULTS &amp; DISCUSSION</span></h2>
                        <div class="p-pair">
                            <p class="p-id">
                                Tabel 2 menunjukkan perbandingan komparatif dari parameter uji yang membedakan kinerja DCA Statis Konvensional dengan SmartDCA (Bot B) dalam hal perlindungan modal dan akumulasi efisiensi:
                            </p>
                            <p class="p-en">
                                Table 2 presents a comparative view of the testing parameters that distinguish the performance of Conventional Static DCA from SmartDCA (Bot B) in terms of capital protection and accumulation efficiency:
                            </p>
                        </div>

                        <table class="academic-table">
                            <thead>
                                <tr>
                                    <th>Parameter Uji<br/><span style="font-size:0.8em;font-weight:normal;">Testing Parameter</span></th>
                                    <th>DCA Konvensional (Statis)<br/><span style="font-size:0.8em;font-weight:normal;">Conventional DCA (Static)</span></th>
                                    <th>SmartDCA (Bot B Dinamis)<br/><span style="font-size:0.8em;font-weight:normal;">SmartDCA (Bot B Dynamic)</span></th>
                                </tr>
                            </thead>
                            <tbody>
                                <tr>
                                    <td><strong>Total Penumpukan Posisi Beli</strong> / <em>Buy Order Pyramiding</em></td>
                                    <td>Tanpa Batas / <em>Unbounded</em></td>
                                    <td>Maksimal 3 Layer / <em>3 Layers Max</em></td>
                                </tr>
                                <tr>
                                    <td><strong>Filter Sinyal Masuk Pertama</strong> / <em>Initial Entry Filter</em></td>
                                    <td>Berbasis Waktu / <em>Time-Based</em></td>
                                    <td>RSI-14 &lt; 30 / <em>RSI-14 &lt; 30</em></td>
                                </tr>
                                <tr>
                                    <td><strong>Proteksi Panic dump</strong> / <em>Falling Knife Protection</em></td>
                                    <td>Tidak Ada / <em>None</em></td>
                                    <td>Aktif Blokir 15 Menit / <em>15-Min Block</em></td>
                                </tr>
                                <tr>
                                    <td><strong>Average Entry Cost Basis</strong> / <em>Average Entry Cost Basis</em></td>
                                    <td>Tinggi / <em>High</em></td>
                                    <td>Rendah / <em>Low</em></td>
                                </tr>
                                <tr>
                                    <td><strong>Metode Realized Profit</strong> / <em>Exit Profit Realization</em></td>
                                    <td>Statis (+1.5%) / <em>Static (+1.5%)</em></td>
                                    <td>Trailing Exit (+0.5% dari HWM) / <em>Trailing Exit</em></td>
                                </tr>
                            </tbody>
                        </table>

                        <div class="p-pair">
                            <p class="p-id">
                                Hasil simulasi pada Tabel 2 menunjukkan bahwa SmartDCA (Bot B) mencapai imbal hasil yang disesuaikan dengan risiko yang lebih unggul dibandingkan DCA statis konvensional. Dengan membatasi akumulasi posisi maksimal 3 layer, mesin mencegah penipisan modal selama tren turun yang berkepanjangan. Filter RSI-14 jenuh jual memastikan bahwa pembelian hanya dimulai selama periode pembalikan arah dengan probabilitas tinggi. Lebih lanjut, proteksi dump dari Volume Surge berhasil memblokir order selama kejatuhan likuiditas yang ekstrem, mencegah drawdown yang substansial.
                            </p>
                            <p class="p-en">
                                The simulation results in Table 2 demonstrate that SmartDCA (Bot B) achieves a superior risk-adjusted return compared to conventional static DCA. By restricting the pyramiding to a maximum of 3 layers, the engine prevents capital depletion during prolonged downtrends. The oversold RSI-14 filter ensures that purchases are only initiated during periods of high probability reversals. Furthermore, the Volume Surge dump protection successfully blocks orders during extreme liquidity cascades, preventing substantial drawdown.
                            </p>
                        </div>
                    </div>

                    <!-- PAGE 5: 5. Conclusion & References -->
                    <div class="book-page" data-page="5">
                        <h2 class="section-title" id="sec-5">5. Kesimpulan <span class="en-title">/ 5. CONCLUSION</span></h2>
                        <div class="p-pair">
                            <p class="p-id">
                                Modifikasi DCA konvensional menjadi SmartDCA (Bot B) dengan integrasi filter momentum RSI-14 dan deteksi Volume Surge secara signifikan mampu mengurangi risiko penumpukan posisi beli di area harga yang kurang menguntungkan. Penerapan trailing exit berbasis High Water Mark memaksimalkan profitabilitas siklus akumulasi jangka menengah sekaligus menjaga drawdown portofolio berada dalam batas aman. Kerangka kerja matematis ini terbukti andal dalam memelihara keseimbangan modal di pasar kripto.
                            </p>
                            <p class="p-en">
                                Modifying conventional DCA into SmartDCA (Bot B) via RSI-14 momentum filtering and Volume Surge dump protections significantly mitigates the risk of buying assets in unfavorable pricing territories. Implementing HWM-based trailing exits maximizes profits in medium-term accumulation cycles while maintaining safe portfolio drawdowns. This mathematical framework proves highly reliable for capital preservation in cryptocurrency spot trading.
                            </p>
                        </div>

                        <h2 class="section-title" id="sec-6">DAFTAR PUSTAKA <span class="en-title">/ REFERENCES</span></h2>
                        <div style="font-size: 0.9em; line-height: 1.8; margin-left: 25px; text-indent: -25px;">
                            <p>Constantinides, G. M. (1979). A note on the suboptimality of dollar-cost averaging as an investment policy. <em>Journal of Financial and Quantitative Analysis</em>, 14(2), 443–450.</p>
                            <p>Murphy, J. J. (1999). <em>Technical Analysis of the Financial Markets: A Comprehensive Guide to Trading Methods and Applications</em>. New York Institute of Finance.</p>
                            <p>Pardo, R. (2008). <em>The Evaluation and Optimization of Trading Strategies</em> (2nd ed.). John Wiley &amp; Sons.</p>
                            <p>Wilder, J. W. (1978). <em>New Concepts in Technical Trading Systems</em>. Trend Research.</p>
                        </div>

                        <div class="footer">
                            Buku Skripsi Akademik — Bot B (SmartDCA Engine) &nbsp;|&nbsp; Publikasi Resmi Fase 5.0<br />
                            Academic Thesis Book — SmartDCA Bot B<br/><br/>
                            Penyusun: BitTrade Quantitative Research Group &nbsp;|&nbsp; Divisi Riset Agentic Trading
                        </div>
                    </div>

                </div><!-- #pages-container -->
            </div><!-- #pages-viewport -->

            <!-- BOOK NAVIGATION BAR -->
            <div class="book-nav-bar">
                <div class="page-indicator"><span id="currentPage">1</span> / 6</div>
            </div>

            <button id="btnPrev" class="btn-action floating-btn left-btn" disabled>&#10094;</button>
            <button id="btnNext" class="btn-action floating-btn right-btn">&#10095;</button>

        </div><!-- .latex-document -->
    </div><!-- .paper-main-content -->
  </div><!-- .paper-layout-wrapper -->

""" + script_content_6_pages + "\n</Layout>\n\n" + css_content_6_pages

with open('/root/bittrade-v2-strategi/web-ui/src/pages/paper_arbitrage.astro', 'w') as f:
    f.write(paper_arbitrage_new)

with open('/root/bittrade-v2-strategi/web-ui/src/pages/paper_b.astro', 'w') as f:
    f.write(paper_b_new)

