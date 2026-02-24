export default function Home() {
  return (
    <main className="flex min-h-screen flex-col items-center justify-center p-24">
      <h1 className="text-4xl font-bold mb-4">{{PROJECT_NAME}}</h1>
      <p className="text-lg text-gray-600">
        Your agentic development environment is ready.
      </p>
      <p className="mt-4 text-sm text-gray-400">
        Run <code className="bg-gray-100 px-2 py-1 rounded">spawn run claude</code> to start building.
      </p>
    </main>
  );
}
