// Oore's service worker. It only shows push notifications for new builds;
// it caches nothing, so the app always loads fresh from the server.

self.addEventListener('install', () => {
  self.skipWaiting()
})

self.addEventListener('activate', (event) => {
  event.waitUntil(self.clients.claim())
})

self.addEventListener('push', (event) => {
  let message = { title: 'Oore', body: 'Something changed.', url: '/' }
  try {
    message = { ...message, ...event.data.json() }
  } catch {
    // An empty or plain-text push still shows something.
  }
  event.waitUntil(
    self.registration.showNotification(message.title, {
      body: message.body,
      icon: '/logo192.png',
      badge: '/logo192.png',
      data: { url: message.url },
      tag: message.url,
    }),
  )
})

self.addEventListener('notificationclick', (event) => {
  event.notification.close()
  const path = event.notification.data?.url ?? '/'
  const target = new URL(path, self.location.origin).href
  event.waitUntil(
    self.clients
      .matchAll({ type: 'window', includeUncontrolled: true })
      .then((windows) => {
        for (const client of windows) {
          if (new URL(client.url).origin === self.location.origin) {
            return client.focus().then(() => client.navigate(target))
          }
        }
        return self.clients.openWindow(target)
      }),
  )
})
