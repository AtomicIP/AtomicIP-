/**
 * Notification preferences — Issue: reduce notification noise
 */

function normalizePreferences(preferences = []) {
  if (!Array.isArray(preferences)) {
    throw new TypeError("preferences must be an array.");
  }

  const normalized = new Map();
  for (const pref of preferences) {
    if (!pref || typeof pref !== "object") continue;
    if (!pref.userId) continue;
    normalized.set(pref.userId, {
      userId: pref.userId,
      email: pref.email !== false,
      push: pref.push !== false,
      sms: pref.sms === true,
      categories: Array.isArray(pref.categories) ? pref.categories : [],
    });
  }
  return normalized;
}

function shouldSendNotification(userPreferences, type, category) {
  if (!userPreferences || typeof userPreferences !== "object") {
    return true;
  }

  const channel = userPreferences[type] ?? true;
  if (channel === false) return false;

  const categories = Array.isArray(userPreferences.categories) ? userPreferences.categories : [];
  if (categories.length === 0) return true;
  return category == null || categories.includes(category);
}

function filterNotificationsForUser(notifications, preferences) {
  if (!Array.isArray(notifications)) {
    throw new TypeError("notifications must be an array.");
  }

  return notifications.filter((notification) => {
    const userPrefs = preferences && preferences[notification.userId];
    if (!userPrefs) return true;
    const category = notification.category;
    return shouldSendNotification(userPrefs, notification.channel, category);
  });
}

module.exports = {
  normalizePreferences,
  shouldSendNotification,
  filterNotificationsForUser,
};
