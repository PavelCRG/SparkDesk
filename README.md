<p align="center"><img src="apps/desktop/ui-react/public/sparkdesk-mark.png" width="104" alt="SparkDesk logo" /></p>

<h1 align="center">SparkDesk</h1>

<p align="center"><strong>Remote desktop for Windows</strong></p>

SparkDesk lets you view and control another Windows computer remotely. Connect using the other computer's SparkDesk ID, then use its mouse and keyboard when the person there enables control.

> **Early preview:** SparkDesk is still being tested. A Windows installer is not published yet.

## Start a session

1. Open SparkDesk on both computers.
2. On the computer you want to access, copy the ID shown on its home screen and share it with the person helping you.
3. The helper enters that ID and sends a connection request.
4. Accept the request on the computer being accessed. The remote screen opens in its own window.
5. Screen viewing starts with mouse and keyboard control off. Turn control on only when needed; turn it off at any time to stop remote input while keeping the screen visible.
6. Close the session window or choose **End session** to disconnect.

## What to expect

- SparkDesk shows the primary monitor. The viewer can be resized or switched to full screen.
- The person at the remote computer accepts every session and controls whether mouse and keyboard input is allowed.
- The home window stays available while a session is open.
- The interface is available in English and Russian.

## Privacy and access

- A SparkDesk ID helps find a computer; knowing the ID does not grant access.
- The owner must accept each connection. Mouse and keyboard control is a separate permission and starts off.
- The owner can revoke control without ending screen viewing. Either person can end the session.
- SparkDesk uses a direct connection between computers. Some routers or firewalls may prevent a direct connection; a relay service is not configured yet.
- Windows may block remote input into elevated apps, sign-in screens and other protected system surfaces.

## Current limits

SparkDesk is an early Windows preview with primary-monitor sharing. The 60 FPS target and performance across two computers have not been verified yet. Connection quality depends on both computers and the network. No public installer is available yet.

<details>
<summary>Русский</summary>

## SparkDesk — удалённый рабочий стол для Windows

Подключайтесь к другому компьютеру по его ID, просматривайте экран и управляйте мышью и клавиатурой, когда человек за этим компьютером разрешит управление.

### Как подключиться

1. Откройте SparkDesk на обоих компьютерах.
2. На компьютере, к которому нужно подключиться, скопируйте ID с главного экрана и передайте его помощнику.
3. Помощник вводит ID и отправляет запрос на подключение.
4. Примите запрос на компьютере, к которому подключаются. Удалённый экран откроется в отдельном окне.
5. Просмотр начинается без управления. Включайте управление мышью и клавиатурой только при необходимости. Его можно отключить, не завершая просмотр.
6. Закройте окно сеанса или нажмите **Завершить сеанс**, чтобы отключиться.

### Доступ и ограничения

- ID помогает найти компьютер, но сам по себе не открывает доступ.
- Каждый запрос нужно принять. Управление мышью и клавиатурой — отдельное разрешение, по умолчанию оно выключено.
- Можно просматривать основной монитор и развернуть окно на весь экран.
- Некоторые маршрутизаторы и сетевые экраны могут блокировать прямое подключение. Ретранслятор пока не настроен.
- Windows может блокировать ввод в приложения с повышенными правами и защищённые системные окна.
- Это ранняя версия: цель 60 FPS и скорость работы между двумя компьютерами ещё не проверены. Установщик пока не опубликован.

</details>

<details>
<summary>Run from source (for contributors)</summary>

Requires Rust 1.92+, Node.js 20.19+ and npm.

```powershell
cd apps/desktop/ui-react
npm ci
$env:SPARKDESK_SIGNALING_URL = "wss://sparkdesk-signaling.obtainable-bloom.workers.dev"
npm run tauri -- dev
```

The example signaling endpoint is temporary and may expire. To run the signaling service locally, start the Worker in one terminal:

```powershell
cd cloud/signaling
npm ci
npm run dev
```

Then point the desktop app to it in another terminal:

```powershell
$env:SPARKDESK_SIGNALING_URL = "ws://127.0.0.1:8787"
cd apps/desktop/ui-react
npm run tauri -- dev
```

</details>
