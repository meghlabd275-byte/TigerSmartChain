// Settings Page - User preferences and account settings
import { useState } from 'react';
import Header from '../components/Header';

interface UserSettings {
  theme: 'light' | 'dark' | 'system';
  currency: string;
  language: string;
  notifications: {
    email: boolean;
    push: boolean;
    sms: boolean;
    newBlocks: boolean;
    transactions: boolean;
    priceAlerts: boolean;
  };
  privacy: {
    showBalance: boolean;
    showTransactions: boolean;
    publicProfile: boolean;
  };
  security: {
    twoFactor: boolean;
    loginAlerts: boolean;
  };
}

export default function SettingsPage() {
  const [settings, setSettings] = useState<UserSettings>({
    theme: 'system',
    currency: 'USD',
    language: 'en',
    notifications: {
      email: true,
      push: true,
      sms: false,
      newBlocks: true,
      transactions: true,
      priceAlerts: false,
    },
    privacy: {
      showBalance: true,
      showTransactions: true,
      publicProfile: false,
    },
    security: {
      twoFactor: false,
      loginAlerts: true,
    },
  });

  const [saved, setSaved] = useState(false);

  const handleSave = () => {
    localStorage.setItem('userSettings', JSON.stringify(settings));
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  const updateSetting = (path: string, value: any) => {
    setSettings((prev: UserSettings) => {
      const newSettings = { ...prev };
      const keys = path.split('.');
      let current: any = newSettings;
      for (let i = 0; i < keys.length - 1; i++) {
        current = current[keys[i]];
      }
      current[keys[keys.length - 1]] = value;
      return newSettings;
    });
  };

  return (
    <div className="min-h-screen bg-gray-50">
      <Header />
      
      <main className="max-w-4xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900">Settings</h1>
          <p className="mt-2 text-gray-600">Manage your account preferences</p>
        </div>

        {/* Appearance */}
        <section className="bg-white rounded-xl shadow-sm border border-gray-200 mb-6">
          <div className="px-6 py-4 border-b border-gray-200">
            <h2 className="text-lg font-semibold text-gray-900">Appearance</h2>
          </div>
          <div className="p-6 space-y-4">
            <div>
              <label className="block text-sm font-medium text-gray-700 mb-2">Theme</label>
              <select
                value={settings.theme}
                onChange={(e) => updateSetting('theme', e.target.value)}
                className="w-full px-4 py-2 border border-gray-300 rounded-lg"
              >
                <option value="light">Light</option>
                <option value="dark">Dark</option>
                <option value="system">System</option>
              </select>
            </div>

            <div>
              <label className="block text-sm font-medium text-gray-700 mb-2">Currency</label>
              <select
                value={settings.currency}
                onChange={(e) => updateSetting('currency', e.target.value)}
                className="w-full px-4 py-2 border border-gray-300 rounded-lg"
              >
                <option value="USD">USD</option>
                <option value="EUR">EUR</option>
                <option value="GBP">GBP</option>
                <option value="JPY">JPY</option>
              </select>
            </div>

            <div>
              <label className="block text-sm font-medium text-gray-700 mb-2">Language</label>
              <select
                value={settings.language}
                onChange={(e) => updateSetting('language', e.target.value)}
                className="w-full px-4 py-2 border border-gray-300 rounded-lg"
              >
                <option value="en">English</option>
                <option value="es">Español</option>
                <option value="fr">Français</option>
                <option value="zh">中文</option>
              </select>
            </div>
          </div>
        </section>

        {/* Notifications */}
        <section className="bg-white rounded-xl shadow-sm border border-gray-200 mb-6">
          <div className="px-6 py-4 border-b border-gray-200">
            <h2 className="text-lg font-semibold text-gray-900">Notifications</h2>
          </div>
          <div className="p-6 space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <div className="font-medium text-gray-900">Email Notifications</div>
                <div className="text-sm text-gray-500">Receive updates via email</div>
              </div>
              <button
                onClick={() => updateSetting('notifications.email', !settings.notifications.email)}
                className={`w-12 h-6 rounded-full transition-colors ${
                  settings.notifications.email ? 'bg-blue-600' : 'bg-gray-300'
                }`}
              >
                <div className={`w-5 h-5 bg-white rounded-full transition-transform ${
                  settings.notifications.email ? 'translate-x-6' : 'translate-x-0.5'
                }`} />
              </button>
            </div>

            <div className="flex items-center justify-between">
              <div>
                <div className="font-medium text-gray-900">New Blocks</div>
                <div className="text-sm text-gray-500">Notify when new blocks are mined</div>
              </div>
              <button
                onClick={() => updateSetting('notifications.newBlocks', !settings.notifications.newBlocks)}
                className={`w-12 h-6 rounded-full transition-colors ${
                  settings.notifications.newBlocks ? 'bg-blue-600' : 'bg-gray-300'
                }`}
              >
                <div className={`w-5 h-5 bg-white rounded-full transition-transform ${
                  settings.notifications.newBlocks ? 'translate-x-6' : 'translate-x-0.5'
                }`} />
              </button>
            </div>

            <div className="flex items-center justify-between">
              <div>
                <div className="font-medium text-gray-900">Transaction Updates</div>
                <div className="text-sm text-gray-500">Notify on transaction status changes</div>
              </div>
              <button
                onClick={() => updateSetting('notifications.transactions', !settings.notifications.transactions)}
                className={`w-12 h-6 rounded-full transition-colors ${
                  settings.notifications.transactions ? 'bg-blue-600' : 'bg-gray-300'
                }`}
              >
                <div className={`w-5 h-5 bg-white rounded-full transition-transform ${
                  settings.notifications.transactions ? 'translate-x-6' : 'translate-x-0.5'
                }`} />
              </button>
            </div>

            <div className="flex items-center justify-between">
              <div>
                <div className="font-medium text-gray-900">Price Alerts</div>
                <div className="text-sm text-gray-500">Get notified on price changes</div>
              </div>
              <button
                onClick={() => updateSetting('notifications.priceAlerts', !settings.notifications.priceAlerts)}
                className={`w-12 h-6 rounded-full transition-colors ${
                  settings.notifications.priceAlerts ? 'bg-blue-600' : 'bg-gray-300'
                }`}
              >
                <div className={`w-5 h-5 bg-white rounded-full transition-transform ${
                  settings.notifications.priceAlerts ? 'translate-x-6' : 'translate-x-0.5'
                }`} />
              </button>
            </div>
          </div>
        </section>

        {/* Privacy */}
        <section className="bg-white rounded-xl shadow-sm border border-gray-200 mb-6">
          <div className="px-6 py-4 border-b border-gray-200">
            <h2 className="text-lg font-semibold text-gray-900">Privacy</h2>
          </div>
          <div className="p-6 space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <div className="font-medium text-gray-900">Show Balance</div>
                <div className="text-sm text-gray-500">Allow others to see your balance</div>
              </div>
              <button
                onClick={() => updateSetting('privacy.showBalance', !settings.privacy.showBalance)}
                className={`w-12 h-6 rounded-full transition-colors ${
                  settings.privacy.showBalance ? 'bg-blue-600' : 'bg-gray-300'
                }`}
              >
                <div className={`w-5 h-5 bg-white rounded-full transition-transform ${
                  settings.privacy.showBalance ? 'translate-x-6' : 'translate-x-0.5'
                }`} />
              </button>
            </div>

            <div className="flex items-center justify-between">
              <div>
                <div className="font-medium text-gray-900">Public Profile</div>
                <div className="text-sm text-gray-500">Make your profile visible to others</div>
              </div>
              <button
                onClick={() => updateSetting('privacy.publicProfile', !settings.privacy.publicProfile)}
                className={`w-12 h-6 rounded-full transition-colors ${
                  settings.privacy.publicProfile ? 'bg-blue-600' : 'bg-gray-300'
                }`}
              >
                <div className={`w-5 h-5 bg-white rounded-full transition-transform ${
                  settings.privacy.publicProfile ? 'translate-x-6' : 'translate-x-0.5'
                }`} />
              </button>
            </div>
          </div>
        </section>

        {/* Security */}
        <section className="bg-white rounded-xl shadow-sm border border-gray-200 mb-6">
          <div className="px-6 py-4 border-b border-gray-200">
            <h2 className="text-lg font-semibold text-gray-900">Security</h2>
          </div>
          <div className="p-6 space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <div className="font-medium text-gray-900">Two-Factor Authentication</div>
                <div className="text-sm text-gray-500">Add extra security to your account</div>
              </div>
              <button className="px-4 py-2 bg-blue-600 text-white rounded-lg hover:bg-blue-700">
                Enable
              </button>
            </div>

            <div className="flex items-center justify-between">
              <div>
                <div className="font-medium text-gray-900">Login Alerts</div>
                <div className="text-sm text-gray-500">Get notified of new login attempts</div>
              </div>
              <button
                onClick={() => updateSetting('security.loginAlerts', !settings.security.loginAlerts)}
                className={`w-12 h-6 rounded-full transition-colors ${
                  settings.security.loginAlerts ? 'bg-blue-600' : 'bg-gray-300'
                }`}
              >
                <div className={`w-5 h-5 bg-white rounded-full transition-transform ${
                  settings.security.loginAlerts ? 'translate-x-6' : 'translate-x-0.5'
                }`} />
              </button>
            </div>
          </div>
        </section>

        {/* Save Button */}
        <div className="flex items-center justify-end gap-4">
          {saved && (
            <span className="text-green-600">Settings saved!</span>
          )}
          <button
            onClick={handleSave}
            className="px-6 py-2 bg-blue-600 text-white rounded-lg font-medium hover:bg-blue-700"
          >
            Save Changes
          </button>
        </div>
      </main>
    </div>
  );
}