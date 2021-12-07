package main

import (
	"context"
	"flag"
	"os"
	"time"

	"k8s.io/client-go/tools/clientcmd"
	"k8s.io/klog/v2"

	clientset "github.com/amurant/ring-go-operator/pkg/generated/clientset/versioned"
	informers "github.com/amurant/ring-go-operator/pkg/generated/informers/externalversions"
	"github.com/amurant/ring-go-operator/pkg/signals"
)

var (
	masterURL  string
	kubeconfig string
)

func getEnv(key, fallback string) string {
	if value, ok := os.LookupEnv(key); ok {
		return value
	}
	return fallback
}

func main() {
	ctx, exit := signals.SetupExitHandler(context.Background())
	defer exit() // Will call os.Exit(...) if errorcode != 0

	klog.InitFlags(nil)
	flag.Parse()

	cfg, err := clientcmd.BuildConfigFromFlags(masterURL, kubeconfig)
	if err != nil {
		klog.Fatalf("Error building kubeconfig: %s", err.Error())
	}

	resourceClientset, err := clientset.NewForConfig(cfg)
	if err != nil {
		klog.Fatalf("Error building example clientset: %s", err.Error())
	}

	inNamespace := getEnv("IN_NAMESPACE", "default")
	outNamespace := getEnv("OUT_NAMESPACE", "default")

	resourceInformerFactory := informers.NewSharedInformerFactoryWithOptions(resourceClientset, time.Second*30, informers.WithNamespace(inNamespace))

	controller := NewController(
		resourceClientset,
		resourceInformerFactory.Amurant().V1().TestResources(),
		outNamespace,
	)

	// notice that there is no need to run Start methods in a separate goroutine. (i.e. go kubeInformerFactory.Start(stopCh)
	// Start method is non-blocking and runs all registered informers in a dedicated goroutine.
	resourceInformerFactory.Start(ctx.Done())

	if err = controller.Run(ctx); err != nil {
		klog.Fatalf("Error running controller: %s", err.Error())
	}
}

func init() {
	flag.StringVar(&kubeconfig, "kubeconfig", "", "Path to a kubeconfig. Only required if out-of-cluster.")
	flag.StringVar(&masterURL, "master", "", "The address of the Kubernetes API server. Overrides any value in kubeconfig. Only required if out-of-cluster.")
}
