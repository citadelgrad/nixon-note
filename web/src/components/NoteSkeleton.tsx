export function NoteSkeleton() {
  return (
    <div className="rounded-xl border border-sage-100 bg-white p-5 animate-pulse">
      {/* Content skeleton */}
      <div className="space-y-3">
        <div className="h-4 bg-sage-100 rounded w-3/4"></div>
        <div className="h-4 bg-sage-100 rounded w-full"></div>
        <div className="h-4 bg-sage-100 rounded w-5/6"></div>
      </div>

      {/* Tags skeleton */}
      <div className="mt-3 flex gap-1.5">
        <div className="h-6 bg-sage-50 rounded-full w-16"></div>
        <div className="h-6 bg-sage-50 rounded-full w-20"></div>
      </div>

      {/* Footer skeleton */}
      <div className="mt-3 flex items-center justify-between">
        <div className="h-4 bg-sage-50 rounded w-24"></div>
        <div className="h-4 bg-sage-50 rounded w-16"></div>
      </div>
    </div>
  )
}
