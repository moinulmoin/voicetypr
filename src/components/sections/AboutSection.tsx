import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { NotebookText, Mail, RefreshCw, CheckCircle, Twitter } from "lucide-react";

export function AboutSection() {
  const handleCheckForUpdates = () => {
    console.log("Checking for updates...");
    // Dummy functionality - just log for now
  };

  const openExternalLink = (url: string) => {
    window.open(url, "_blank", "noopener,noreferrer");
  };

  return (
    <div className="p-6">
      <h2 className="text-lg font-semibold text-gray-900 dark:text-gray-100 mb-6">About VoiceTypr</h2>
      
      <div className="space-y-6">
        {/* App Info Section */}
        <div className="space-y-4">
          {/* Version */}
          <div className="flex items-center gap-3">
            <p className="text-sm text-gray-600 dark:text-gray-400">Version</p>
            <p className="text-base font-medium">0.1.0</p>
          </div>
          
          {/* License Status */}
          <div className="flex items-center gap-3">
            <p className="text-sm text-gray-600 dark:text-gray-400">License</p>
            <Badge variant="default" className="bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-100">
              <CheckCircle className="w-3 h-3 mr-1" />
              Active
            </Badge>
          </div>
        </div>

        {/* Links Section */}
        <div className="flex items-center gap-6 mt-8">
            <button
              onClick={() => openExternalLink("https://voicetypr.com/changelog")}
              className="flex items-center gap-2 text-sm text-gray-900 dark:text-gray-100 hover:text-gray-600 dark:hover:text-gray-400 hover:underline underline-offset-4"
            >
              <NotebookText className="w-4 h-4" />
              Changelog
            </button>
            <button
              onClick={() => openExternalLink("mailto:support@voicetypr.com")}
              className="flex items-center gap-2 text-sm text-gray-900 dark:text-gray-100 hover:text-gray-600 dark:hover:text-gray-400 hover:underline underline-offset-4"
            >
              <Mail className="w-4 h-4" />
              support@voicetypr.com
            </button>
            <button
              onClick={() => openExternalLink("https://twitter.com/voicetypr")}
              className="flex items-center gap-2 text-sm text-gray-900 dark:text-gray-100 hover:text-gray-600 dark:hover:text-gray-400 hover:underline underline-offset-4"
            >
              <Twitter className="w-4 h-4" />
              @voicetypr
            </button>
        </div>

        {/* Check for Updates Button */}
        <div className="mt-12 flex justify-center">
          <Button 
            size="sm" 
            variant="outline" 
            onClick={handleCheckForUpdates}
            className="h-8"
          >
            <RefreshCw className="w-3 h-3 mr-1" />
            Check for Updates
          </Button>
        </div>
      </div>
    </div>
  );
}