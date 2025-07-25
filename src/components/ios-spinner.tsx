import { cn } from "@/lib/utils"

interface IOSSpinnerProps {
  size?: number
  className?: string
}

export default function IOSSpinner({ size = 20, className }: IOSSpinnerProps) {
  return (
    <>
      <div className={cn("relative inline-flex items-center justify-center", className)} style={{ width: size, height: size }}>
        {[...Array(12)].map((_, i) => (
          <div
            key={i}
            className="absolute bg-current rounded-full"
            style={{
              width: size * 0.08,
              height: size * 0.25,
              left: "50%",
              top: "50%",
              transformOrigin: `center`,
              transform: `translateX(-50%) translateY(-50%) rotate(${i * 30}deg) translateY(${-size * 0.375}px)`,
              opacity: 1 - i * 0.08,
              animation: "ios-spin 1s linear infinite",
              animationDelay: `${(11 - i) * -0.083}s`,
            }}
          />
        ))}
      </div>
    </>
  )
}
