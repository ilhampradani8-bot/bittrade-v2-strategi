# 🛠️ BotTrade Technical Stack & Architecture

## 🌐 Frontend (Admin & User Dashboard)
| Technology | Function | Location |
| :--- | :--- | :--- |
| **Next.js 14+** | Framework utama untuk UI Dashboard yang cepat dan SEO Friendly. | `/admin`, `/frontend` |
| **TailwindCSS** | Styling utility-first untuk desain premium & responsive. | `index.css` / Config |
| **Lucide React** | Library icon set yang konsisten dan ringan. | Seluruh UI |
| **Socket.io Client** | Mendengarkan log realtime dari server untuk tampilan Terminal. | `EnginePage.tsx` |
| **JWT** | Menyimpan sesi login admin secara aman di LocalStorage/Cookie. | `LoginPage.tsx` |

## ⚙️ Backend (Server API)
| Technology | Function | Location |
| :--- | :--- | :--- |
| **Rust (Axum)** | Web framework performa tinggi untuk mengelola API & routing. | `backend/src/server/api.rs` |
| **SQLx** | Library database toolkit yang mendukung Compile-time check untuk PostgreSQL. | `backend/src/server/admin.rs` |
| **Socketioxide** | Implementasi Socket.io di Rust untuk streaming data realtime (logs/chat). | `backend/src/server/api.rs` |
| **Bcrypt** | Hashing password user & admin secara aman. | `postgres_auth_hub.rs` |
| **Chrono** | Pengelolaan data waktu (timestamp) crypto yang akurat. | Seluruh Backend |

## 🚀 Trading Engine (Engine24am)
| Technology | Function | Location |
| :--- | :--- | :--- |
| **Tokio** | Asynchronous runtime untuk menjalankan ribuan bot (worker) secara paralel. | `backend/src/main.rs` |
| **Polars** | Library Dataframe tercepat untuk analisa teknikal & kalkulasi strategi. | `engine/src/analysis.rs` (Planned) |
| **CCXT (PyO3)** | Bridge ke library Python CCXT untuk akses ke 100+ Exchange. | `api_exchange/ccxt_provider.rs` |
| **PostgreSQL** | Database utama untuk menyimpan strategi, user, dan history trade. | Managed Service |

## 📁 File Structure Key
- `backend/src/strategies/`: Tempat penyimpanan logika strategi (DCA, Grid, dll).
- `engine/src/bot24jam/`: (Planned) Modul khusus yang berisi worker trading tiap bot.
- `backend/src/server/admin.rs`: Modul kontrol admin (Live SQL logic).
- `.env`: Konfigurasi sensitif (DB URL, JWT Secret, API Keys).

---
*Dokumentasi ini menjelaskan alur data dari UI hingga eksekusi mesin.*
