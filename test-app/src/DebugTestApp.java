import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.stream.*;

/**
 * Test application for jdwp-mcp feature testing.
 *
 * Scenarios:
 * 1. Hanging thread (deadlock between two locks)
 * 2. Busy loop with inspectable variables
 * 3. Exception throwing
 * 4. Field modification
 * 5. Object with fields for inspection
 * 6. Methods for eval (toString, getters)
 *
 * Run with:
 *   javac src/DebugTestApp.java -d build
 *   java -agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:5005 -cp build DebugTestApp
 */
public class DebugTestApp {

    // === Scenario 1: Deadlock ===
    private static final Object lockA = new Object();
    private static final Object lockB = new Object();

    // === Scenario 2: Counter with inspectable state ===
    private static final AtomicInteger requestCount = new AtomicInteger(0);

    // === Scenario 4: Watchable field ===
    private static String status = "STARTING";

    // === Scenario 5: Inspectable object ===
    static class User {
        String name;
        int age;
        boolean active;
        List<String> roles;

        User(String name, int age, boolean active, List<String> roles) {
            this.name = name;
            this.age = age;
            this.active = active;
            this.roles = roles;
        }

        // === Scenario 6: Methods for eval ===
        @Override
        public String toString() {
            return "User{name=" + name + ", age=" + age + ", active=" + active + "}";
        }

        public String getName() {
            return name;
        }

        public boolean isActive() {
            return active;
        }
    }

    static class Order {
        int id;
        String product;
        double price;
        User buyer;

        Order(int id, String product, double price, User buyer) {
            this.id = id;
            this.product = product;
            this.price = price;
            this.buyer = buyer;
        }

        @Override
        public String toString() {
            return "Order#" + id + "(" + product + ", $" + price + ")";
        }
    }

    // === Scenario 7: Deep hierarchy for inspect stress test (issue #3/#4) ===
    static abstract class BaseEntity {
        long id;
        String createdAt;
        String updatedAt;
        String createdBy;
        String updatedBy;
        boolean deleted;
        int version;
        Map<String, Object> metadata;
        List<String> tags;
        String description;
    }

    static abstract class AuditableEntity extends BaseEntity {
        String auditLog;
        List<String> changeHistory;
        Map<String, String> permissions;
        String ownerGroup;
        boolean locked;
        String lockReason;
        long lockTimestamp;
        String lastAccessedBy;
        long lastAccessedAt;
        int accessCount;
    }

    static class ComplexProduct extends AuditableEntity {
        String sku;
        String name;
        String brand;
        String category;
        double price;
        double weight;
        int stockCount;
        boolean available;
        List<String> images;
        Map<String, String> attributes;
        List<ComplexProduct> relatedProducts;
        String warehouse;
        double rating;
        int reviewCount;

        ComplexProduct(String sku, String name) {
            this.sku = sku;
            this.name = name;
            this.id = ThreadLocalRandom.current().nextLong();
            this.createdAt = "2024-01-01";
            this.updatedAt = "2024-06-15";
            this.metadata = new HashMap<>();
            this.metadata.put("origin", "factory");
            this.metadata.put("batch", "2024-Q2");
            this.tags = List.of("electronics", "sale");
            this.changeHistory = List.of("created", "updated", "reviewed");
            this.permissions = Map.of("read", "all", "write", "admin");
            this.attributes = Map.of("color", "black", "size", "M");
            this.images = List.of("front.jpg", "back.jpg", "side.jpg");
            this.relatedProducts = new ArrayList<>();
            this.price = 99.99;
            this.weight = 0.5;
            this.stockCount = 42;
            this.available = true;
            this.rating = 4.5;
            this.reviewCount = 128;
        }

        @Override
        public String toString() {
            return "ComplexProduct{sku=" + sku + ", name=" + name + "}";
        }
    }

    // Accessible from worker thread for inspect testing
    static volatile ComplexProduct testProduct;

    public static void main(String[] args) throws Exception {
        System.out.println("DebugTestApp started. JDWP listening on port 5005.");
        System.out.println("Scenarios: deadlock, counter, exceptions, field watch, object inspect");
        System.out.println();

        status = "RUNNING";

        // Scenario 1: Deadlock — two threads grabbing locks in opposite order
        Thread deadlockThread1 = new Thread(() -> {
            synchronized (lockA) {
                sleep(100); // give thread2 time to grab lockB
                System.out.println("[deadlock-1] waiting for lockB...");
                synchronized (lockB) {
                    System.out.println("[deadlock-1] got both locks (should not happen)");
                }
            }
        }, "deadlock-thread-1");

        Thread deadlockThread2 = new Thread(() -> {
            synchronized (lockB) {
                sleep(100); // give thread1 time to grab lockA
                System.out.println("[deadlock-2] waiting for lockA...");
                synchronized (lockA) {
                    System.out.println("[deadlock-2] got both locks (should not happen)");
                }
            }
        }, "deadlock-thread-2");

        deadlockThread1.start();
        deadlockThread2.start();

        // Scenario 2 + 3 + 4 + 5: Worker loop
        Thread workerThread = new Thread(() -> {
            User alice = new User("Alice", 30, true, List.of("admin", "user"));
            User bob = new User("Bob", 25, false, List.of("user"));

            // Scenario 7: Complex object for inspect stress test
            testProduct = new ComplexProduct("SKU-001", "Widget Pro");
            testProduct.relatedProducts.add(new ComplexProduct("SKU-002", "Widget Lite"));
            testProduct.relatedProducts.add(new ComplexProduct("SKU-003", "Widget Max"));

            while (true) {
                int count = requestCount.incrementAndGet();

                // Scenario 5: Create objects for inspection
                Order order = new Order(count, "Widget-" + count, 9.99 * count, alice);

                // Scenario 2: Breakpoint target — inspectable locals
                processOrder(order, count);

                // Scenario 4: Field modification (watchpoint target)
                if (count % 5 == 0) {
                    status = "PROCESSING_BATCH_" + count;
                }

                // Scenario 3: Exception every 7th iteration
                if (count % 7 == 0) {
                    try {
                        riskyOperation(count);
                    } catch (Exception e) {
                        System.out.println("[worker] caught: " + e.getMessage());
                    }
                }

                sleep(2000);
            }
        }, "worker-thread");
        workerThread.start();

        System.out.println("All threads started. Deadlock will occur in ~100ms.");
        System.out.println("Worker processes orders every 2s. Press Ctrl+C to stop.");

        // Keep main alive
        Thread.currentThread().join();
    }

    /**
     * Breakpoint target: has local variables to inspect.
     * Line numbers are stable for breakpoint testing.
     */
    static void processOrder(Order order, int count) {    // line ~132
        String summary = order.product + " for " + order.buyer.name;
        double total = order.price * 1.2; // with tax
        boolean isLargeOrder = total > 50.0;

        if (isLargeOrder) {                                // line ~136
            System.out.println("[worker] large order #" + count + ": " + summary + " = $" + total);
        } else {
            System.out.println("[worker] order #" + count + ": " + summary + " = $" + total);
        }
    }

    /**
     * Throws exception for exception-breakpoint testing.
     */
    static void riskyOperation(int count) {                // line ~144
        if (count % 7 == 0) {
            throw new RuntimeException("Simulated error at count=" + count);
        }
    }

    private static void sleep(long ms) {
        try { Thread.sleep(ms); } catch (InterruptedException e) { Thread.currentThread().interrupt(); }
    }
}
